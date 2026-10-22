use ghost_proto::{
    CcdLoad, CoreLoad, CpuSnapshot, DiskSnapshot, Drive, DriveState, GpuSnapshot, MemSnapshot,
    NetIface, NetSnapshot, ProcNode, ProcTree, ProcUsage, Reading, SensorSnapshot,
};

use super::Wave;
use crate::collect::{ProcSample, Source};
use crate::sampler::Sampler;

/// A [`Sampler`] over the fake sources.
pub fn sampler() -> Sampler {
    Sampler::new(
        Box::new(FakeCpu::default()),
        Box::new(FakeMem::default()),
        Box::new(FakeSensors::default()),
        Box::new(FakeGpu::default()),
        Box::new(FakeDisks::default()),
        Box::new(FakeNet::default()),
        Box::new(FakeProcs::default()),
    )
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
const PACKAGE_W: Wave = Wave::new(28.0, 142.0, 35.0);
const IGPU_BUSY_PCT: Wave = Wave::new(0.0, 18.0, 55.0);

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
        let cores = (0..CCD_COUNT * CORES_PER_CCD)
            .map(|core| {
                let cpus = vec![core, core + SMT_SIBLING_OFFSET];
                CoreLoad {
                    load: mean(cpus.iter().map(|&cpu| threads[usize::from(cpu)])),
                    cpus,
                }
            })
            .collect();
        CpuSnapshot {
            model: CPU_MODEL.to_owned(),
            load: mean(threads.iter().copied()),
            threads,
            ccds,
            cores,
            freq_mhz: FREQ_MHZ.at(self.samples) as u32,
            package_w: Some(PACKAGE_W.at(self.samples)),
            igpu_busy_pct: Some(IGPU_BUSY_PCT.at(self.samples)),
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
        MemSnapshot {
            total_kib: MEM_TOTAL_KIB,
            used_kib: share_of(MEM_TOTAL_KIB, MEM_USED_SHARE.at(self.samples)),
            swap_total_kib: SWAP_TOTAL_KIB,
            swap_used_kib: share_of(SWAP_TOTAL_KIB, SWAP_USED_SHARE.at(self.samples)),
        }
    }
}

fn share_of(total: u64, share: f32) -> u64 {
    (total as f64 * f64::from(share)) as u64
}

// The real collector builds the tree on every fifth of its scans, which fall on every second sample.
const TREE_EVERY_SAMPLES: u64 = 10;
// The reference mockup's KTHREADS.
const KERNEL_THREADS: u32 = 436;

struct TreeProcess {
    pid: u32,
    ppid: u32,
    name: &'static str,
    cpu: f32,
    rss_kib: u64,
}

const fn tree_process(
    pid: u32,
    ppid: u32,
    name: &'static str,
    cpu: f32,
    rss_kib: u64,
) -> TreeProcess {
    TreeProcess {
        pid,
        ppid,
        name,
        cpu,
        rss_kib,
    }
}

// Rows of the reference mockup's process table (its buildTree), by pid as the real tree is sent. The
// slack's parent is not listed, as when a parent exits between scans, so it hangs from the root.
const TREE: [TreeProcess; 40] = [
    tree_process(1, 0, "systemd", 0.0, 21_852),
    tree_process(1278, 1, "auditd", 0.0, 3_612),
    tree_process(1280, 1278, "sedispatch", 0.0, 3_940),
    tree_process(1303, 1, "dbus-broker-lau", 0.0, 7_496),
    tree_process(1309, 1303, "dbus-broker", 0.0, 11_116),
    tree_process(1322, 1, "NetworkManager", 0.0, 23_672),
    tree_process(1339, 1, "udisksd", 0.0, 18_212),
    tree_process(1340, 1, "upowerd", 0.0, 14_160),
    tree_process(1371, 1, "systemd-logind", 0.0, 11_680),
    tree_process(1541, 1, "tailscaled", 0.0, 41_024),
    tree_process(1743, 1, "sddm", 0.0, 27_080),
    tree_process(2074, 1743, "sddm-helper", 0.0, 28_176),
    tree_process(2081, 1, "systemd", 0.0, 16_796),
    tree_process(2129, 2081, "pipewire", 0.0, 29_668),
    tree_process(2130, 2081, "wireplumber", 0.0, 34_840),
    tree_process(2131, 2081, "pipewire-pulse", 0.1, 42_684),
    tree_process(2136, 2074, "startplasma-way", 0.0, 44_212),
    tree_process(2398, 2081, "kwin_wayland_wr", 0.0, 21_568),
    tree_process(2411, 2398, "kwin_wayland", 6.8, 1_003_412),
    tree_process(2494, 2411, "Xwayland", 2.0, 283_268),
    tree_process(581452, 581450, "slack", 0.1, 257_844),
    tree_process(704437, 2081, "bwrap", 0.0, 1_628),
    tree_process(704440, 704437, "spotify", 0.9, 486_256),
    tree_process(704541, 704440, "spotify", 10.1, 528_864),
    tree_process(707794, 2081, "plasmashell", 2.8, 1_016_164),
    tree_process(713026, 2081, "konsole", 0.5, 149_692),
    tree_process(758925, 713026, "zsh", 0.0, 8_228),
    tree_process(760517, 758925, "claude", 1.7, 511_488),
    tree_process(783628, 2081, "qs", 3.9, 376_592),
    tree_process(792997, 1, "mount.ntfs", 1.8, 157_360),
    tree_process(804665, 2081, "bwrap", 0.0, 2_368),
    tree_process(804675, 804665, "bwrap", 0.0, 1_584),
    tree_process(804676, 804675, "firefox-bin", 7.9, 1_002_444),
    tree_process(804754, 804676, "forkserver", 0.0, 29_504),
    tree_process(805048, 804754, "Isolated", 3.2, 435_588),
    tree_process(808918, 2081, "thorium", 4.8, 644_924),
    tree_process(808938, 808918, "thorium", 0.0, 18_484),
    tree_process(809079, 808938, "thorium", 3.2, 267_348),
    tree_process(813424, 808938, "thorium", 85.1, 445_312),
    tree_process(814125, 783628, "cava", 0.5, 10_680),
];

fn fake_tree() -> ProcTree {
    ProcTree {
        kernel_threads: KERNEL_THREADS,
        procs: TREE
            .iter()
            .map(|process| ProcNode {
                pid: process.pid,
                ppid: process.ppid,
                name: process.name.to_owned(),
                cpu: process.cpu,
                rss_kib: process.rss_kib,
            })
            .collect(),
    }
}

/// The busiest processes, and on the first sample and every tenth after it the process tree.
#[derive(Default)]
struct FakeProcs {
    samples: u64,
}

impl Source<ProcSample> for FakeProcs {
    fn sample(&mut self) -> ProcSample {
        let tree = self
            .samples
            .is_multiple_of(TREE_EVERY_SAMPLES)
            .then(fake_tree);
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
        ProcSample { top, tree }
    }
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

struct FakeIface {
    name: &'static str,
    rx_bps: Wave,
    tx_bps: Wave,
}

// The wired link carries the default route; the two tunnels read `unknown` live, and count as up.
const DEFAULT_IFACE: &str = "enp14s0";
const IFACES: [FakeIface; 3] = [
    FakeIface {
        name: DEFAULT_IFACE,
        rx_bps: Wave::new(20_000.0, 48_000_000.0, 33.0),
        tx_bps: Wave::new(8_000.0, 6_500_000.0, 47.0),
    },
    FakeIface {
        name: "tailscale0",
        rx_bps: Wave::new(0.0, 900_000.0, 52.0),
        tx_bps: Wave::new(0.0, 300_000.0, 61.0),
    },
    FakeIface {
        name: "tun0",
        rx_bps: Wave::new(0.0, 2_400_000.0, 39.0),
        tx_bps: Wave::new(0.0, 700_000.0, 44.0),
    },
];

#[derive(Default)]
struct FakeNet {
    samples: u64,
}

impl Source<NetSnapshot> for FakeNet {
    fn sample(&mut self) -> NetSnapshot {
        self.samples += 1;
        let interfaces: Vec<NetIface> = IFACES
            .iter()
            .map(|iface| NetIface {
                name: iface.name.to_owned(),
                oper_up: true,
                rx_bps: iface.rx_bps.at(self.samples) as u64,
                tx_bps: iface.tx_bps.at(self.samples) as u64,
            })
            .collect();
        let headline = &interfaces[0];
        NetSnapshot {
            default_iface: Some(DEFAULT_IFACE.to_owned()),
            rx_bps: headline.rx_bps,
            tx_bps: headline.tx_bps,
            interfaces,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use ghost_proto::{ServerMsg, topics};

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
    fn each_of_the_16_cores_averages_a_thread_and_its_sibling() {
        let cpu = FakeCpu::default().sample();

        assert_eq!(cpu.cores.len(), 16);
        for (index, core) in cpu.cores.iter().enumerate() {
            let first = u16::try_from(index).unwrap();
            assert_eq!(core.cpus, [first, first + 16]);
            let threads = [cpu.threads[index], cpu.threads[index + 16]];
            assert_eq!(core.load, (threads[0] + threads[1]) / 2.0);
        }
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
    fn the_tree_comes_on_the_first_sample_and_every_tenth_after() {
        let mut procs = FakeProcs::default();

        let with_tree: Vec<bool> = (0..21).map(|_| procs.sample().tree.is_some()).collect();

        let expected: Vec<bool> = (0..21).map(|index| index % 10 == 0).collect();
        assert_eq!(with_tree, expected);
    }

    #[test]
    fn the_tree_has_a_root_an_orphan_kernel_threads_and_busy_processes() {
        let tree = FakeProcs::default().sample().tree.unwrap();

        assert_eq!(tree.procs.len(), 40);
        assert_eq!(tree.kernel_threads, 436);
        let pids: BTreeSet<u32> = tree.procs.iter().map(|process| process.pid).collect();
        assert_eq!(pids.len(), tree.procs.len(), "every pid is listed once");
        assert!(pids.contains(&1));
        let orphans: Vec<&str> = tree
            .procs
            .iter()
            .filter(|process| process.pid != 1 && !pids.contains(&process.ppid))
            .map(|process| process.name.as_str())
            .collect();
        assert_eq!(orphans, ["slack"]);
        let busy = tree
            .procs
            .iter()
            .filter(|process| process.cpu > 4.0)
            .count();
        assert!(
            busy >= 4,
            "only {busy} processes are busy enough to be labelled"
        );
    }

    #[test]
    fn the_sampler_publishes_the_fake_tree() {
        let wanted = [topics::PROCS_TREE.to_owned()].into_iter().collect();

        let messages = sampler().tick(&mut History::new(1), &wanted);

        let tree: ProcTree = data(&messages, topics::PROCS_TREE);
        assert_eq!(tree.procs.len(), 40);
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
