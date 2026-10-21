use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ghost_proto::{DiskSnapshot, Drive, DriveState};
use ghostd::collect::{DiskCollector, FsStat};

use super::FakeRoot;

pub const ONE_SECOND: Duration = Duration::from_secs(1);
pub const SERIAL_WIDTH: usize = 20;
pub const MODEL_WIDTH: usize = 40;
pub const LIVE_SIZE_SECTORS: u64 = 488_397_168;
pub const SAMPLES_PER_REFRESH: u64 = 60;
pub const BYTES_PER_SECTOR: u64 = 512;
pub const SECTORS_PER_SAMPLE: u64 = 2_000;
pub const STEADY_READ_BPS: u64 = SECTORS_PER_SAMPLE * BYTES_PER_SECTOR;
pub const OTHER_DRIVES_SECTORS: u64 = 900_000_000;

pub const KINGSTON_MODEL: &str = "KINGSTON SKC2500M8250G";
pub const KINGSTON_SERIAL: &str = "50026B7684E5C533";
pub const WD_MODEL: &str = "WDC WDS960G2G0C-00AJM0";
pub const WD_SERIAL: &str = "213823802844";
pub const XPG_MODEL: &str = "XPG GAMMIX S70 BLADE";
pub const XPG_SERIAL: &str = "2O412LA45EG7";

pub const ROOT_USED_PCT: f32 = 42.5;
pub const DATA_USED_PCT: f32 = 17.0;

pub struct FakeDisk {
    name: String,
    dev: String,
    model: &'static str,
    serial: Option<&'static str>,
    wwid: Option<&'static str>,
    state: &'static str,
    size_sectors: u64,
    removable: bool,
    has_device: bool,
    partitions: Vec<(String, String)>,
}

impl FakeDisk {
    pub fn nvme(name: &str, dev: &str, model: &'static str, serial: &'static str) -> Self {
        Self {
            name: name.to_owned(),
            dev: dev.to_owned(),
            model,
            serial: Some(serial),
            wwid: None,
            state: "live",
            size_sectors: LIVE_SIZE_SECTORS,
            removable: false,
            has_device: true,
            partitions: Vec::new(),
        }
    }

    pub fn sata(name: &str, dev: &str, model: &'static str, serial: Option<&'static str>) -> Self {
        Self {
            serial,
            state: "running",
            ..Self::nvme(name, dev, model, "")
        }
    }

    pub fn wwid(self, wwid: &'static str) -> Self {
        Self {
            wwid: Some(wwid),
            ..self
        }
    }

    pub fn state(self, state: &'static str) -> Self {
        Self { state, ..self }
    }

    pub fn size(self, size_sectors: u64) -> Self {
        Self {
            size_sectors,
            ..self
        }
    }

    pub fn removable(self) -> Self {
        Self {
            removable: true,
            ..self
        }
    }

    /// Loop, md and nbd devices have no hardware behind them, so sysfs gives them no `device` entry.
    pub fn virtual_device(self) -> Self {
        Self {
            has_device: false,
            ..self
        }
    }

    pub fn partition(mut self, name: &str, dev: &str) -> Self {
        self.partitions.push((name.to_owned(), dev.to_owned()));
        self
    }

    /// Sysfs pads an NVMe controller's `model` and `serial` with spaces, so the fake does too.
    pub fn write(&self, fake: &FakeRoot) {
        let dir = format!("sys/block/{}", self.name);
        fake.write(&format!("{dir}/dev"), &format!("{}\n", self.dev));
        fake.write(&format!("{dir}/size"), &format!("{}\n", self.size_sectors));
        fake.write(
            &format!("{dir}/removable"),
            &format!("{}\n", u8::from(self.removable)),
        );
        if self.has_device {
            fake.write(&format!("{dir}/device/state"), &format!("{}\n", self.state));
            fake.write(
                &format!("{dir}/device/model"),
                &format!("{:<MODEL_WIDTH$}\n", self.model),
            );
        }
        if let Some(serial) = self.serial.filter(|_| self.has_device) {
            fake.write(
                &format!("{dir}/device/serial"),
                &format!("{serial:<SERIAL_WIDTH$}\n"),
            );
        }
        if let Some(wwid) = self.wwid.filter(|_| self.has_device) {
            fake.write(&format!("{dir}/device/wwid"), &format!("{wwid}\n"));
        }
        for (name, dev) in &self.partitions {
            fake.write(&format!("{dir}/{name}/dev"), &format!("{dev}\n"));
        }
    }
}

pub fn mount_line(dev: &str, mount_point: &str, fstype: &str, source: &str) -> String {
    format!("36 1 {dev} / {mount_point} rw,relatime shared:1 - {fstype} {source} rw\n")
}

pub fn write_mounts(fake: &FakeRoot, lines: &[String]) {
    fake.write("proc/self/mountinfo", &lines.concat());
}

pub fn diskstats_line(dev: &str, name: &str, sectors_read: u64, sectors_written: u64) -> String {
    let (major, minor) = dev.split_once(':').unwrap();
    format!("{major:>4} {minor:>7} {name} 10 0 {sectors_read} 5 10 0 {sectors_written} 5 0 10 10\n")
}

/// Counts the whole disk's sectors and gives its partition a large count the collector must ignore.
pub fn write_diskstats(fake: &FakeRoot, name: &str, dev: &str, read: u64, written: u64) {
    let partition = diskstats_line(dev, &format!("{name}p1"), 999_999_999, 999_999_999);
    fake.write(
        "proc/diskstats",
        &format!("{}{partition}", diskstats_line(dev, name, read, written)),
    );
}

#[derive(Default)]
pub struct FakeFs {
    pub used_pct: BTreeMap<String, f32>,
    pub calls: Arc<Mutex<Vec<String>>>,
}

impl FakeFs {
    pub fn with_usage(usage: &[(&str, f32)]) -> Self {
        Self {
            used_pct: usage
                .iter()
                .map(|(mount, used_pct)| ((*mount).to_owned(), *used_pct))
                .collect(),
            ..Self::default()
        }
    }
}

impl FsStat for FakeFs {
    fn used_pct(&self, mount: &str) -> Option<f32> {
        self.calls.lock().unwrap().push(mount.to_owned());
        self.used_pct.get(mount).copied()
    }
}

pub fn collector(fake: &FakeRoot) -> DiskCollector {
    DiskCollector::new(&fake.root(), Box::new(FakeFs::default()), &BTreeMap::new())
}

pub fn sample(fake: &FakeRoot) -> DiskSnapshot {
    collector(fake).sample_after(ONE_SECOND)
}

pub fn drive<'a>(snapshot: &'a DiskSnapshot, kernel_name: &str) -> &'a Drive {
    snapshot
        .drives
        .iter()
        .find(|drive| drive.kernel_name == kernel_name)
        .unwrap()
}

pub fn kernel_names(snapshot: &DiskSnapshot) -> Vec<&str> {
    snapshot
        .drives
        .iter()
        .map(|drive| drive.kernel_name.as_str())
        .collect()
}

pub fn ids_and_states(snapshot: &DiskSnapshot) -> Vec<(&str, DriveState)> {
    snapshot
        .drives
        .iter()
        .map(|drive| (drive.id.as_str(), drive.state.clone()))
        .collect()
}

/// Where the kernel put a drive: the enumeration order decides the name and the device numbers.
#[derive(Clone, Copy)]
pub struct Slot {
    pub name: &'static str,
    pub dev: &'static str,
    pub system_partition_dev: &'static str,
}

pub const SLOT_0: Slot = Slot {
    name: "nvme0n1",
    dev: "259:0",
    system_partition_dev: "259:3",
};
pub const SLOT_1: Slot = Slot {
    name: "nvme1n1",
    dev: "259:5",
    system_partition_dev: "259:6",
};
pub const SLOT_2: Slot = Slot {
    name: "nvme2n1",
    dev: "259:7",
    system_partition_dev: "259:10",
};

/// The real box's NVMe drives. The XPG holds `/` on a btrfs partition, which `mountinfo` reports
/// under an anonymous `0:35`, and the Kingston is mounted by the desktop. The WD is dead.
pub fn nvme_machine(kingston: Slot, wd: Slot, xpg: Slot) -> FakeRoot {
    let fake = FakeRoot::new();
    let kingston_partition = format!("{}p3", kingston.name);
    let xpg_partition = format!("{}p3", xpg.name);
    FakeDisk::nvme(kingston.name, kingston.dev, KINGSTON_MODEL, KINGSTON_SERIAL)
        .partition(&kingston_partition, kingston.system_partition_dev)
        .write(&fake);
    FakeDisk::nvme(wd.name, wd.dev, WD_MODEL, WD_SERIAL)
        .state("dead")
        .size(0)
        .write(&fake);
    FakeDisk::nvme(xpg.name, xpg.dev, XPG_MODEL, XPG_SERIAL)
        .partition(&xpg_partition, xpg.system_partition_dev)
        .write(&fake);
    write_mounts(
        &fake,
        &[
            mount_line("0:35", "/", "btrfs", &format!("/dev/{xpg_partition}")),
            mount_line(
                kingston.system_partition_dev,
                "/run/media/gtkacz/E6BEBF34BEBEFBDD",
                "fuseblk",
                &format!("/dev/{kingston_partition}"),
            ),
        ],
    );
    fake
}
