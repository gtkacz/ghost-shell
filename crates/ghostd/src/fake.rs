//! Synthetic telemetry for `ghostd run --fake`, shaped like the reference machine so the shell can
//! be developed and tested anywhere: two CCDs of 16 threads, a dead WD Green, an unreadable sensor
//! and an RTX 4080 SUPER. Every value follows a sine over the sample count, so runs repeat exactly.

use std::f64::consts::TAU;
use std::future::pending;
use std::time::{SystemTime, UNIX_EPOCH};

use ghost_proto::{
    CcdLoad, CpuSnapshot, DiskSnapshot, Drive, DriveState, GpuSnapshot, HostSnapshot, MemSnapshot,
    ProcUsage, Reading, SensorSnapshot, topics,
};

use crate::collect::Source;
use crate::sampler::Sampler;
use crate::server::Feed;

/// A [`Sampler`] over the fake sources.
pub fn sampler() -> Sampler {
    Sampler::new(
        Box::new(FakeCpu::default()),
        Box::new(FakeMem::default()),
        Box::new(FakeSensors::default()),
        Box::new(FakeGpu::default()),
        Box::new(FakeDisks::default()),
    )
}

/// The fake feeds, one per feed topic.
pub fn feeds() -> Vec<Feed> {
    vec![host()]
}

/// A sine between `low` and `high` that repeats every `period` samples, starting `phase` of a
/// period in.
#[derive(Clone, Copy)]
struct Wave {
    low: f32,
    high: f32,
    period: f64,
    phase: f64,
}

impl Wave {
    const fn new(low: f32, high: f32, period: f64) -> Self {
        Self {
            low,
            high,
            period,
            phase: 0.0,
        }
    }

    const fn shifted(self, phase: f64) -> Self {
        Self { phase, ..self }
    }

    fn at(self, sample: u64) -> f32 {
        let angle = TAU * (sample as f64 / self.period + self.phase);
        let unit = (angle.sin() + 1.0) / 2.0;
        self.low + (self.high - self.low) * unit as f32
    }
}

fn mean(values: impl ExactSizeIterator<Item = f32>) -> f32 {
    let count = values.len();
    if count == 0 {
        return 0.0;
    }
    values.sum::<f32>() / count as f32
}

const CPU_MODEL: &str = "AMD Ryzen 9 9950X3D 16-Core Processor";
const CCD_COUNT: u16 = 2;
const CORES_PER_CCD: u16 = 8;
// Logical CPU n and n + 16 are the two threads of one core.
const SMT_SIBLING_OFFSET: u16 = 16;
const THREAD_COUNT: u16 = CCD_COUNT * CORES_PER_CCD * 2;
const VCACHE_CCD: u16 = 0;
const VCACHE_L3_KIB: u32 = 98_304;
const PLAIN_L3_KIB: u32 = 32_768;
const THREAD_LOAD: Wave = Wave::new(3.0, 97.0, 40.0);
// Each thread runs the same wave a little later than the one before, so the load ripples across
// both CCDs instead of every cell moving together.
const THREAD_PHASE_STEP: f64 = 1.0 / THREAD_COUNT as f64;
const FREQ_MHZ: Wave = Wave::new(3600.0, 5700.0, 25.0);

#[derive(Default)]
struct FakeCpu {
    samples: u64,
}

impl Source<CpuSnapshot> for FakeCpu {
    fn sample(&mut self) -> CpuSnapshot {
        self.samples += 1;
        let threads: Vec<f32> = (0..THREAD_COUNT)
            .map(|cpu| {
                THREAD_LOAD
                    .shifted(f64::from(cpu) * THREAD_PHASE_STEP)
                    .at(self.samples)
            })
            .collect();
        let ccds = (0..CCD_COUNT)
            .map(|ccd| {
                let cpus = ccd_cpus(ccd);
                CcdLoad {
                    id: ccd as u8,
                    load: mean(cpus.iter().map(|&cpu| threads[usize::from(cpu)])),
                    cpus,
                    l3_kib: if ccd == VCACHE_CCD {
                        VCACHE_L3_KIB
                    } else {
                        PLAIN_L3_KIB
                    },
                    vcache: ccd == VCACHE_CCD,
                }
            })
            .collect();
        CpuSnapshot {
            model: CPU_MODEL.to_owned(),
            load: mean(threads.iter().copied()),
            threads,
            ccds,
            freq_mhz: FREQ_MHZ.at(self.samples) as u32,
        }
    }
}

fn ccd_cpus(ccd: u16) -> Vec<u16> {
    let first_core = ccd * CORES_PER_CCD;
    let cores = first_core..first_core + CORES_PER_CCD;
    cores
        .clone()
        .chain(cores.map(|core| core + SMT_SIBLING_OFFSET))
        .collect()
}

// 91.9 GiB of the 96 GB fitted are usable.
const MEM_TOTAL_KIB: u64 = 96_364_134;
const SWAP_TOTAL_KIB: u64 = 8 * 1024 * 1024;
const MEM_USED_SHARE: Wave = Wave::new(0.25, 0.55, 90.0);
const SWAP_USED_SHARE: Wave = Wave::new(0.0, 0.08, 120.0);
const PROCESS_CPU: Wave = Wave::new(0.2, 22.0, 30.0);
const PROCESS_PHASE_STEP: f64 = 0.17;

struct FakeProcess {
    pid: u32,
    name: &'static str,
    mem: f32,
}

const PROCESSES: [FakeProcess; 5] = [
    FakeProcess {
        pid: 2214,
        name: "firefox",
        mem: 6.4,
    },
    FakeProcess {
        pid: 1630,
        name: "kwin_wayland",
        mem: 1.1,
    },
    FakeProcess {
        pid: 3307,
        name: "qs",
        mem: 0.6,
    },
    FakeProcess {
        pid: 1702,
        name: "plasmashell",
        mem: 0.9,
    },
    FakeProcess {
        pid: 3290,
        name: "ghostd",
        mem: 0.1,
    },
];

#[derive(Default)]
struct FakeMem {
    samples: u64,
}

impl Source<MemSnapshot> for FakeMem {
    fn sample(&mut self) -> MemSnapshot {
        self.samples += 1;
        let mut top: Vec<ProcUsage> = PROCESSES
            .iter()
            .enumerate()
            .map(|(index, process)| ProcUsage {
                pid: process.pid,
                name: process.name.to_owned(),
                cpu: PROCESS_CPU
                    .shifted(index as f64 * PROCESS_PHASE_STEP)
                    .at(self.samples),
                mem: process.mem,
            })
            .collect();
        top.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
        MemSnapshot {
            total_kib: MEM_TOTAL_KIB,
            used_kib: share_of(MEM_TOTAL_KIB, MEM_USED_SHARE.at(self.samples)),
            swap_total_kib: SWAP_TOTAL_KIB,
            swap_used_kib: share_of(SWAP_TOTAL_KIB, SWAP_USED_SHARE.at(self.samples)),
            top,
        }
    }
}

fn share_of(total: u64, share: f32) -> u64 {
    (total as f64 * f64::from(share)) as u64
}

struct FakeSensor {
    id: &'static str,
    label: &'static str,
    wave: Wave,
}

const TEMPS: [FakeSensor; 7] = [
    FakeSensor {
        id: "k10temp/Tctl",
        label: "CPU TCTL",
        wave: Wave::new(42.0, 93.0, 40.0),
    },
    FakeSensor {
        id: "k10temp/Tccd1",
        label: "CCD0",
        wave: Wave::new(40.0, 88.0, 40.0),
    },
    FakeSensor {
        id: "k10temp/Tccd2",
        label: "CCD1",
        wave: Wave::new(38.0, 84.0, 40.0),
    },
    FakeSensor {
        id: "nct6799/SYSTIN",
        label: "MOBO",
        wave: Wave::new(31.0, 36.0, 200.0),
    },
    FakeSensor {
        id: "spd5118/1",
        label: "DIMM 1",
        wave: Wave::new(38.0, 47.0, 150.0),
    },
    FakeSensor {
        id: "spd5118/2",
        label: "DIMM 2",
        wave: Wave::new(39.0, 48.0, 150.0),
    },
    FakeSensor {
        id: "r8169_0_e00:00/temp1",
        label: "NIC",
        wave: Wave::new(44.0, 52.0, 180.0),
    },
];

const FANS: [FakeSensor; 2] = [
    FakeSensor {
        id: "nct6799/fan2",
        label: "CPU FAN",
        wave: Wave::new(900.0, 1900.0, 60.0),
    },
    FakeSensor {
        id: "nct6799/fan3",
        label: "CHASSIS FAN",
        wave: Wave::new(420.0, 520.0, 60.0),
    },
];

impl FakeSensor {
    fn reading(&self, sample: u64) -> Reading {
        Reading {
            id: self.id.to_owned(),
            label: self.label.to_owned(),
            value: Some(self.wave.at(sample)),
        }
    }
}

#[derive(Default)]
struct FakeSensors {
    samples: u64,
}

impl Source<SensorSnapshot> for FakeSensors {
    fn sample(&mut self) -> SensorSnapshot {
        self.samples += 1;
        // NVMe readings follow the drives, so `attach_temps` finds each drive's temperature by
        // serial exactly as it does on real hardware.
        let drive_temps = DRIVES.iter().filter_map(|drive| {
            let value = match drive.temp {
                NvmeTemp::Absent => return None,
                NvmeTemp::Unreadable => None,
                NvmeTemp::Reads(wave) => Some(wave.at(self.samples)),
            };
            Some(Reading {
                id: format!("nvme/{}", drive.serial),
                label: drive.model.to_owned(),
                value,
            })
        });
        SensorSnapshot {
            temps: TEMPS
                .iter()
                .map(|sensor| sensor.reading(self.samples))
                .chain(drive_temps)
                .collect(),
            fans: FANS
                .iter()
                .map(|sensor| sensor.reading(self.samples))
                .collect(),
        }
    }
}

const GPU_NAME: &str = "NVIDIA GeForce RTX 4080 SUPER";
const GPU_VRAM_TOTAL_MIB: u32 = 16_376;
const GPU_POWER_LIMIT_W: f32 = 320.0;
const GPU_LOAD: Wave = Wave::new(0.0, 100.0, 50.0);
const GPU_VRAM_SHARE: Wave = Wave::new(0.15, 0.7, 80.0);
const GPU_TEMP_C: Wave = Wave::new(38.0, 78.0, 50.0);
const GPU_POWER_W: Wave = Wave::new(22.0, 305.0, 50.0);
const GPU_FAN_PCT: Wave = Wave::new(0.0, 65.0, 50.0);

#[derive(Default)]
struct FakeGpu {
    samples: u64,
}

impl Source<GpuSnapshot> for FakeGpu {
    fn sample(&mut self) -> GpuSnapshot {
        self.samples += 1;
        let vram_share = GPU_VRAM_SHARE.at(self.samples);
        GpuSnapshot {
            present: true,
            name: GPU_NAME.to_owned(),
            load: GPU_LOAD.at(self.samples),
            vram_used_mib: (f64::from(GPU_VRAM_TOTAL_MIB) * f64::from(vram_share)) as u32,
            vram_total_mib: GPU_VRAM_TOTAL_MIB,
            temp_c: GPU_TEMP_C.at(self.samples),
            power_w: GPU_POWER_W.at(self.samples),
            power_limit_w: GPU_POWER_LIMIT_W,
            fan_pct: GPU_FAN_PCT.at(self.samples),
        }
    }
}

const READ_BPS: Wave = Wave::new(0.0, 420_000_000.0, 20.0);
const WRITE_BPS: Wave = Wave::new(0.0, 180_000_000.0, 28.0);
const DRIVE_PHASE_STEP: f64 = 0.31;

#[derive(Clone, Copy)]
enum NvmeTemp {
    /// Not an NVMe drive, so there is no reading at all.
    Absent,
    /// The reading exists but fails, as on a dead controller.
    Unreadable,
    Reads(Wave),
}

struct FakeDrive {
    model: &'static str,
    serial: &'static str,
    kernel_name: &'static str,
    state: DriveState,
    mounts: &'static [&'static str],
    used_pct: Option<f32>,
    temp: NvmeTemp,
}

// Listed root first, then live, dead and unmounted drives, the order the real collector reports.
const DRIVES: [FakeDrive; 5] = [
    FakeDrive {
        model: "XPG GAMMIX S70 BLADE",
        serial: "2N4829FAKE01",
        kernel_name: "nvme2n1",
        state: DriveState::Live,
        mounts: &["/", "/home"],
        used_pct: Some(61.8),
        temp: NvmeTemp::Reads(Wave::new(41.0, 58.0, 70.0)),
    },
    FakeDrive {
        model: "KINGSTON SKC2500M8250G",
        serial: "50026B7FAKE02",
        kernel_name: "nvme0n1",
        state: DriveState::Live,
        mounts: &["/run/media/user/Windows"],
        used_pct: Some(47.3),
        temp: NvmeTemp::Reads(Wave::new(36.0, 44.0, 90.0)),
    },
    FakeDrive {
        model: "WD GREEN 960G",
        serial: "22163FAKE03",
        kernel_name: "nvme1n1",
        state: DriveState::Dead,
        mounts: &[],
        used_pct: None,
        temp: NvmeTemp::Unreadable,
    },
    FakeDrive {
        model: "WDC WD10EZEX-08WN4A0",
        serial: "WD-WCC6YFAKE04",
        kernel_name: "sda",
        state: DriveState::Unmounted,
        mounts: &[],
        used_pct: None,
        temp: NvmeTemp::Absent,
    },
    FakeDrive {
        model: "ST3000VX010-2H916L",
        serial: "Z4FFAKE05",
        kernel_name: "sdb",
        state: DriveState::Unmounted,
        mounts: &[],
        used_pct: None,
        temp: NvmeTemp::Absent,
    },
];

#[derive(Default)]
struct FakeDisks {
    samples: u64,
}

impl Source<DiskSnapshot> for FakeDisks {
    fn sample(&mut self) -> DiskSnapshot {
        self.samples += 1;
        let drives = DRIVES
            .iter()
            .enumerate()
            .map(|(index, drive)| {
                let is_active = drive.state == DriveState::Live;
                let phase = index as f64 * DRIVE_PHASE_STEP;
                let rate = |wave: Wave| {
                    if is_active {
                        wave.shifted(phase).at(self.samples) as u64
                    } else {
                        0
                    }
                };
                Drive {
                    id: format!("{}#{}", drive.model, drive.serial),
                    label: drive.model.to_owned(),
                    model: drive.model.to_owned(),
                    kernel_name: drive.kernel_name.to_owned(),
                    state: drive.state.clone(),
                    mounts: drive.mounts.iter().map(|&mount| mount.to_owned()).collect(),
                    used_pct: drive.used_pct,
                    temp_c: None,
                    read_bps: rate(READ_BPS),
                    write_bps: rate(WRITE_BPS),
                }
            })
            .collect();
        DiskSnapshot { drives }
    }
}

const HOSTNAME: &str = "tkacz-workstation";
const KERNEL: &str = "7.2.11-200.fc44.x86_64";
// The mockup's uptime of 2:14:07.
const UPTIME_S: u64 = 8047;

fn host() -> Feed {
    Feed::new(topics::HOST, |context| async move {
        let now_s = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since_epoch| since_epoch.as_secs());
        context.publisher.publish(
            topics::HOST,
            &HostSnapshot {
                hostname: HOSTNAME.to_owned(),
                kernel: KERNEL.to_owned(),
                boot_time_s: now_s.saturating_sub(UPTIME_S),
            },
        );
        pending::<()>().await;
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use ghost_proto::ServerMsg;

    use super::*;
    use crate::history::History;

    fn data<T: serde::de::DeserializeOwned>(messages: &[ServerMsg], wanted: &str) -> T {
        messages
            .iter()
            .find_map(|message| match message {
                ServerMsg::Data { topic, data, .. } if topic == wanted => {
                    Some(serde_json::from_value(data.clone()).unwrap())
                }
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn two_ccds_share_all_32_threads_once() {
        let cpu = FakeCpu::default().sample();

        assert_eq!(cpu.threads.len(), 32);
        assert_eq!(cpu.ccds.len(), 2);
        assert_eq!(cpu.ccds[0].cpus.len(), 16);
        let all: BTreeSet<u16> = cpu.ccds.iter().flat_map(|ccd| ccd.cpus.clone()).collect();
        assert_eq!(all, (0..32).collect());
        assert!(cpu.ccds[0].vcache && !cpu.ccds[1].vcache);
    }

    #[test]
    fn machine_specifics_survive_the_sampler() {
        let every_topic = topics::BUILTIN
            .iter()
            .map(|&topic| topic.to_owned())
            .collect();

        let messages = sampler().tick(&mut History::new(1), &every_topic);

        let disks: DiskSnapshot = data(&messages, topics::DISKS);
        let dead: Vec<_> = disks
            .drives
            .iter()
            .filter(|drive| drive.state == DriveState::Dead)
            .collect();
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0].label, "WD GREEN 960G");
        assert!(
            disks.drives[0].temp_c.is_some(),
            "NVMe temps attach by serial"
        );

        let sensors: SensorSnapshot = data(&messages, topics::SENSORS);
        assert_eq!(
            sensors
                .temps
                .iter()
                .filter(|reading| reading.value.is_none())
                .count(),
            1
        );

        let gpu: GpuSnapshot = data(&messages, topics::GPU);
        assert!(gpu.present);
        assert_eq!(gpu.name, "NVIDIA GeForce RTX 4080 SUPER");
    }

    #[test]
    fn every_value_stays_finite_and_in_range() {
        let mut cpu = FakeCpu::default();
        let mut gpu = FakeGpu::default();
        for _ in 0..500 {
            let cpu = cpu.sample();
            assert!(
                cpu.threads
                    .iter()
                    .chain([&cpu.load])
                    .all(|load| (0.0..=100.0).contains(load))
            );
            let gpu = gpu.sample();
            assert!((0.0..=100.0).contains(&gpu.load));
            assert!(gpu.vram_used_mib <= gpu.vram_total_mib);
        }
    }
}
