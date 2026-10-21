mod common;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::FakeRoot;
use ghost_proto::{DiskSnapshot, Drive, DriveState, Reading, SensorSnapshot};
use ghostd::collect::{DiskCollector, FsStat, attach_temps};

const ONE_SECOND: Duration = Duration::from_secs(1);
const SERIAL_WIDTH: usize = 20;
const MODEL_WIDTH: usize = 40;
const LIVE_SIZE_SECTORS: u64 = 488_397_168;
const SAMPLES_PER_REFRESH: u64 = 60;
const BYTES_PER_SECTOR: u64 = 512;
const SECTORS_PER_SAMPLE: u64 = 2_000;
const STEADY_READ_BPS: u64 = SECTORS_PER_SAMPLE * BYTES_PER_SECTOR;
const OTHER_DRIVES_SECTORS: u64 = 900_000_000;

const KINGSTON_MODEL: &str = "KINGSTON SKC2500M8250G";
const KINGSTON_SERIAL: &str = "50026B7684E5C533";
const WD_MODEL: &str = "WDC WDS960G2G0C-00AJM0";
const WD_SERIAL: &str = "213823802844";
const XPG_MODEL: &str = "XPG GAMMIX S70 BLADE";
const XPG_SERIAL: &str = "2O412LA45EG7";

const ROOT_USED_PCT: f32 = 42.5;
const DATA_USED_PCT: f32 = 17.0;

struct FakeDisk {
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
    fn nvme(name: &str, dev: &str, model: &'static str, serial: &'static str) -> Self {
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

    fn sata(name: &str, dev: &str, model: &'static str, serial: Option<&'static str>) -> Self {
        Self {
            serial,
            state: "running",
            ..Self::nvme(name, dev, model, "")
        }
    }

    fn wwid(self, wwid: &'static str) -> Self {
        Self {
            wwid: Some(wwid),
            ..self
        }
    }

    fn state(self, state: &'static str) -> Self {
        Self { state, ..self }
    }

    fn size(self, size_sectors: u64) -> Self {
        Self {
            size_sectors,
            ..self
        }
    }

    fn removable(self) -> Self {
        Self {
            removable: true,
            ..self
        }
    }

    /// Loop, md and nbd devices have no hardware behind them, so sysfs gives them no `device` entry.
    fn virtual_device(self) -> Self {
        Self {
            has_device: false,
            ..self
        }
    }

    fn partition(mut self, name: &str, dev: &str) -> Self {
        self.partitions.push((name.to_owned(), dev.to_owned()));
        self
    }

    /// Sysfs pads an NVMe controller's `model` and `serial` with spaces, so the fake does too.
    fn write(&self, fake: &FakeRoot) {
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

fn mount_line(dev: &str, mount_point: &str, fstype: &str, source: &str) -> String {
    format!("36 1 {dev} / {mount_point} rw,relatime shared:1 - {fstype} {source} rw\n")
}

fn write_mounts(fake: &FakeRoot, lines: &[String]) {
    fake.write("proc/self/mountinfo", &lines.concat());
}

fn diskstats_line(dev: &str, name: &str, sectors_read: u64, sectors_written: u64) -> String {
    let (major, minor) = dev.split_once(':').unwrap();
    format!("{major:>4} {minor:>7} {name} 10 0 {sectors_read} 5 10 0 {sectors_written} 5 0 10 10\n")
}

/// Counts the whole disk's sectors and gives its partition a large count the collector must ignore.
fn write_diskstats(fake: &FakeRoot, name: &str, dev: &str, read: u64, written: u64) {
    let partition = diskstats_line(dev, &format!("{name}p1"), 999_999_999, 999_999_999);
    fake.write(
        "proc/diskstats",
        &format!("{}{partition}", diskstats_line(dev, name, read, written)),
    );
}

#[derive(Default)]
struct FakeFs {
    used_pct: BTreeMap<String, f32>,
    calls: Arc<Mutex<Vec<String>>>,
}

impl FakeFs {
    fn with_usage(usage: &[(&str, f32)]) -> Self {
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

fn collector(fake: &FakeRoot) -> DiskCollector {
    DiskCollector::new(&fake.root(), Box::new(FakeFs::default()), &BTreeMap::new())
}

fn sample(fake: &FakeRoot) -> DiskSnapshot {
    collector(fake).sample_after(ONE_SECOND)
}

fn drive<'a>(snapshot: &'a DiskSnapshot, kernel_name: &str) -> &'a Drive {
    snapshot
        .drives
        .iter()
        .find(|drive| drive.kernel_name == kernel_name)
        .unwrap()
}

fn kernel_names(snapshot: &DiskSnapshot) -> Vec<&str> {
    snapshot
        .drives
        .iter()
        .map(|drive| drive.kernel_name.as_str())
        .collect()
}

fn ids_and_states(snapshot: &DiskSnapshot) -> Vec<(&str, DriveState)> {
    snapshot
        .drives
        .iter()
        .map(|drive| (drive.id.as_str(), drive.state.clone()))
        .collect()
}

/// Where the kernel put a drive: the enumeration order decides the name and the device numbers.
#[derive(Clone, Copy)]
struct Slot {
    name: &'static str,
    dev: &'static str,
    system_partition_dev: &'static str,
}

const SLOT_0: Slot = Slot {
    name: "nvme0n1",
    dev: "259:0",
    system_partition_dev: "259:3",
};
const SLOT_1: Slot = Slot {
    name: "nvme1n1",
    dev: "259:5",
    system_partition_dev: "259:6",
};
const SLOT_2: Slot = Slot {
    name: "nvme2n1",
    dev: "259:7",
    system_partition_dev: "259:10",
};

/// The real box's NVMe drives. The XPG holds `/` on a btrfs partition, which `mountinfo` reports
/// under an anonymous `0:35`, and the Kingston is mounted by the desktop. The WD is dead.
fn nvme_machine(kingston: Slot, wd: Slot, xpg: Slot) -> FakeRoot {
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

#[test]
fn dead_nvme_is_dead() {
    let fake = FakeRoot::new();
    FakeDisk::nvme("nvme1n1", "259:5", WD_MODEL, WD_SERIAL)
        .state("dead")
        .partition("nvme1n1p1", "259:6")
        .write(&fake);
    FakeDisk::nvme("nvme3n1", "259:11", XPG_MODEL, XPG_SERIAL)
        .size(0)
        .partition("nvme3n1p1", "259:12")
        .write(&fake);
    // A stale mount must not resurrect a drive the kernel has given up on.
    write_mounts(
        &fake,
        &[
            mount_line("259:6", "/stale", "ext4", "/dev/nvme1n1p1"),
            mount_line("259:12", "/stale2", "ext4", "/dev/nvme3n1p1"),
        ],
    );
    write_diskstats(&fake, "nvme1n1", "259:5", 100, 100);
    let fs = FakeFs {
        used_pct: BTreeMap::from([("/stale".to_owned(), 50.0), ("/stale2".to_owned(), 50.0)]),
        ..FakeFs::default()
    };
    let mut disks = DiskCollector::new(&fake.root(), Box::new(fs), &BTreeMap::new());

    disks.sample_after(ONE_SECOND);
    write_diskstats(&fake, "nvme1n1", "259:5", 5_000, 5_000);
    let snapshot = disks.sample_after(ONE_SECOND);

    assert_eq!(snapshot.drives.len(), 2);
    for drive in &snapshot.drives {
        assert_eq!(drive.state, DriveState::Dead, "{}", drive.kernel_name);
        assert!(drive.mounts.is_empty());
        assert_eq!(drive.used_pct, None);
        assert_eq!((drive.read_bps, drive.write_bps), (0, 0));
    }
}

#[test]
fn identity_survives_renumbering() {
    let before = nvme_machine(SLOT_0, SLOT_1, SLOT_2);
    let after = nvme_machine(SLOT_2, SLOT_0, SLOT_1);

    let before = sample(&before);
    let after = sample(&after);

    assert_eq!(
        ids_and_states(&before),
        [
            ("XPG GAMMIX S70 BLADE#2O412LA45EG7", DriveState::Live),
            ("KINGSTON SKC2500M8250G#50026B7684E5C533", DriveState::Live),
            ("WDC WDS960G2G0C-00AJM0#213823802844", DriveState::Dead),
        ]
    );
    assert_eq!(ids_and_states(&before), ids_and_states(&after));
    assert_ne!(kernel_names(&before), kernel_names(&after));
}

#[test]
fn same_model_different_serial_distinct() {
    let fake = FakeRoot::new();
    FakeDisk::nvme("nvme0n1", "259:0", XPG_MODEL, "SERIAL-A").write(&fake);
    FakeDisk::nvme("nvme1n1", "259:5", XPG_MODEL, "SERIAL-B").write(&fake);
    let labels = BTreeMap::from([("SERIAL-B".to_owned(), "Scratch".to_owned())]);

    let snapshot = DiskCollector::new(&fake.root(), Box::new(FakeFs::default()), &labels)
        .sample_after(ONE_SECOND);

    let identities: Vec<_> = snapshot
        .drives
        .iter()
        .map(|drive| {
            (
                drive.id.as_str(),
                drive.label.as_str(),
                drive.model.as_str(),
            )
        })
        .collect();
    assert_eq!(
        identities,
        [
            ("XPG GAMMIX S70 BLADE#SERIAL-A", XPG_MODEL, XPG_MODEL),
            ("XPG GAMMIX S70 BLADE#SERIAL-B", "Scratch", XPG_MODEL),
        ]
    );
}

#[test]
fn serial_falls_back_to_wwid_then_kernel_name() {
    let fake = FakeRoot::new();
    FakeDisk::sata("sda", "8:0", "WDC WD10EZEX-00W", None)
        .wwid("naa.50014ee0598941f2")
        .write(&fake);
    FakeDisk::sata("sdb", "8:16", "ST3000VX010-2E31", None).write(&fake);
    FakeDisk::sata("sdc", "8:32", "SATA SSD", Some("   ")).write(&fake);

    let snapshot = sample(&fake);

    let ids: Vec<_> = snapshot
        .drives
        .iter()
        .map(|drive| drive.id.as_str())
        .collect();
    assert_eq!(
        ids,
        [
            "SATA SSD#sdc",
            "ST3000VX010-2E31#sdb",
            "WDC WD10EZEX-00W#naa.50014ee0598941f2",
        ]
    );
}

#[test]
fn virtual_block_devices_are_not_drives() {
    let fake = FakeRoot::new();
    for (index, name) in ["loop0", "zram0", "ram0", "dm-0"].into_iter().enumerate() {
        FakeDisk::sata(name, &format!("7:{index}"), "virtual", None).write(&fake);
    }
    FakeDisk::nvme("nvme0n1", "259:0", XPG_MODEL, XPG_SERIAL).write(&fake);

    let snapshot = sample(&fake);

    assert_eq!(kernel_names(&snapshot), ["nvme0n1"]);
}

#[test]
fn devices_without_a_device_entry_are_not_drives() {
    let fake = FakeRoot::new();
    FakeDisk::sata("nbd0", "43:0", "nbd", None)
        .size(0)
        .virtual_device()
        .write(&fake);
    FakeDisk::sata("md0", "9:0", "md", None)
        .virtual_device()
        .write(&fake);
    FakeDisk::nvme("nvme0n1", "259:0", XPG_MODEL, XPG_SERIAL).write(&fake);

    let snapshot = sample(&fake);

    assert_eq!(kernel_names(&snapshot), ["nvme0n1"]);
}

#[test]
fn empty_removable_drives_are_not_drives() {
    let fake = FakeRoot::new();
    FakeDisk::sata("sr0", "11:0", "DVD-RW", Some("DVD-1"))
        .removable()
        .size(0)
        .write(&fake);
    FakeDisk::sata("sdc", "8:32", "USB STICK", Some("USB-1"))
        .removable()
        .write(&fake);

    let snapshot = sample(&fake);

    assert_eq!(kernel_names(&snapshot), ["sdc"]);
    assert_eq!(snapshot.drives[0].state, DriveState::Unmounted);
}

#[test]
fn a_fixed_drive_with_no_capacity_is_dead() {
    let fake = FakeRoot::new();
    FakeDisk::sata("sda", "8:0", "WDC WD10EZEX-00W", Some("WD-1"))
        .size(0)
        .write(&fake);

    let snapshot = sample(&fake);

    assert_eq!(snapshot.drives[0].state, DriveState::Dead);
}

#[test]
fn ordering_root_live_dead_unmounted() {
    let fake = FakeRoot::new();
    FakeDisk::sata("sda", "8:0", "WDC WD10EZEX-00W", Some("WD-1"))
        .partition("sda1", "8:1")
        .write(&fake);
    FakeDisk::nvme("nvme0n1", "259:0", WD_MODEL, WD_SERIAL)
        .state("dead")
        .size(0)
        .write(&fake);
    FakeDisk::nvme("nvme1n1", "259:5", "ZZZ DRIVE", "S-LATE")
        .partition("nvme1n1p1", "259:6")
        .write(&fake);
    FakeDisk::nvme("nvme2n1", "259:7", "AAA DRIVE", "S-EARLY")
        .partition("nvme2n1p1", "259:8")
        .write(&fake);
    FakeDisk::sata("sdb", "8:16", "ST3000VX010-2E31", Some("Z7319X8A"))
        .partition("sdb1", "8:17")
        .write(&fake);
    write_mounts(
        &fake,
        &[
            mount_line("259:6", "/mnt/late", "ext4", "/dev/nvme1n1p1"),
            mount_line("259:8", "/mnt/early", "ext4", "/dev/nvme2n1p1"),
            mount_line("8:17", "/", "ext4", "/dev/sdb1"),
        ],
    );

    let snapshot = sample(&fake);

    assert_eq!(
        kernel_names(&snapshot),
        ["sdb", "nvme2n1", "nvme1n1", "nvme0n1", "sda"]
    );
    let states: Vec<_> = snapshot
        .drives
        .iter()
        .map(|drive| drive.state.clone())
        .collect();
    assert_eq!(
        states,
        [
            DriveState::Live,
            DriveState::Live,
            DriveState::Live,
            DriveState::Dead,
            DriveState::Unmounted,
        ]
    );
}

#[test]
fn mounts_match_by_device_number_or_source() {
    let fake = FakeRoot::new();
    FakeDisk::nvme("nvme2n1", "259:7", XPG_MODEL, XPG_SERIAL)
        .partition("nvme2n1p1", "259:8")
        .partition("nvme2n1p2", "259:9")
        .partition("nvme2n1p3", "259:10")
        .write(&fake);
    FakeDisk::nvme("nvme0n1", "259:0", KINGSTON_MODEL, KINGSTON_SERIAL)
        .partition("nvme0n1p3", "259:3")
        .write(&fake);
    write_mounts(
        &fake,
        &[
            // btrfs reports an anonymous device number, so only the source ties it to the drive.
            mount_line("0:35", "/home", "btrfs", "/dev/nvme2n1p3"),
            mount_line("0:35", "/", "btrfs", "/dev/nvme2n1p3"),
            mount_line("259:8", "/boot/efi", "vfat", "/dev/nvme2n1p1"),
            mount_line("259:9", "/boot", "ext4", "/dev/nvme2n1p2"),
            mount_line("259:9", "/boot", "ext4", "/dev/nvme2n1p2"),
            mount_line("0:29", "/run", "tmpfs", "tmpfs"),
            mount_line(
                "259:3",
                r"/run/media/gtkacz/My\040Disk",
                "fuseblk",
                "/dev/nvme0n1p3",
            ),
            // Optional fields are absent here, so the separator follows the mount options directly.
            "37 1 0:40 / /dev/hugepages rw - hugetlbfs hugetlbfs rw\n".to_owned(),
            "garbled line\n".to_owned(),
        ],
    );

    let snapshot = sample(&fake);

    assert_eq!(
        drive(&snapshot, "nvme2n1").mounts,
        ["/", "/boot", "/boot/efi", "/home"]
    );
    assert_eq!(
        drive(&snapshot, "nvme0n1").mounts,
        ["/run/media/gtkacz/My Disk"]
    );
}

#[test]
fn io_rates_from_diskstats() {
    let fake = FakeRoot::new();
    FakeDisk::nvme("nvme0n1", "259:0", XPG_MODEL, XPG_SERIAL).write(&fake);
    write_diskstats(&fake, "nvme0n1", "259:0", 1_000_000, 500_000);
    let mut disks = collector(&fake);

    let first = disks.sample_after(ONE_SECOND);
    write_diskstats(&fake, "nvme0n1", "259:0", 1_002_000, 501_000);
    let second = disks.sample_after(ONE_SECOND);
    write_diskstats(&fake, "nvme0n1", "259:0", 1_004_000, 501_000);
    let third = disks.sample_after(2 * ONE_SECOND);

    assert_eq!(
        (first.drives[0].read_bps, first.drives[0].write_bps),
        (0, 0)
    );
    assert_eq!(second.drives[0].read_bps, 1_024_000);
    assert_eq!(second.drives[0].write_bps, 512_000);
    assert_eq!(third.drives[0].read_bps, 2_000 * BYTES_PER_SECTOR / 2);
    assert_eq!(third.drives[0].write_bps, 0);
}

#[test]
fn io_rates_saturate_when_counters_go_backwards() {
    let fake = FakeRoot::new();
    FakeDisk::nvme("nvme0n1", "259:0", XPG_MODEL, XPG_SERIAL).write(&fake);
    write_diskstats(&fake, "nvme0n1", "259:0", 1_000_000, 500_000);
    let mut disks = collector(&fake);
    disks.sample_after(ONE_SECOND);

    write_diskstats(&fake, "nvme0n1", "259:0", 10, 10);
    let after_reset = disks.sample_after(ONE_SECOND);
    let without_time = disks.sample_after(Duration::ZERO);

    assert_eq!(after_reset.drives[0].read_bps, 0);
    assert_eq!(after_reset.drives[0].write_bps, 0);
    assert_eq!(without_time.drives[0].read_bps, 0);
}

#[test]
fn io_rate_carries_over_a_rescan_unless_the_drive_changed() {
    let fake = FakeRoot::new();
    FakeDisk::nvme("nvme0n1", "259:0", XPG_MODEL, XPG_SERIAL).write(&fake);
    let mut disks = collector(&fake);

    // The sample at index `SAMPLES_PER_REFRESH` is the first to rescan the devices.
    let read_bps: Vec<u64> = (0..2 * SAMPLES_PER_REFRESH)
        .map(|sample| {
            write_diskstats(&fake, "nvme0n1", "259:0", sample * SECTORS_PER_SAMPLE, 0);
            disks.sample_after(ONE_SECOND).drives[0].read_bps
        })
        .collect();

    assert_eq!(read_bps[0], 0);
    assert!(
        read_bps[1..].iter().all(|bps| *bps == STEADY_READ_BPS),
        "{read_bps:?}"
    );

    // A different drive now answers to the same kernel name, with counters of its own.
    FakeDisk::nvme("nvme0n1", "259:0", XPG_MODEL, "OTHER-SERIAL").write(&fake);
    write_diskstats(&fake, "nvme0n1", "259:0", OTHER_DRIVES_SECTORS, 0);
    let swapped = disks.sample_after(ONE_SECOND);
    write_diskstats(
        &fake,
        "nvme0n1",
        "259:0",
        OTHER_DRIVES_SECTORS + SECTORS_PER_SAMPLE,
        0,
    );
    let steady = disks.sample_after(ONE_SECOND);

    assert_eq!(swapped.drives[0].id, "XPG GAMMIX S70 BLADE#OTHER-SERIAL");
    assert_eq!(swapped.drives[0].read_bps, 0);
    assert_eq!(steady.drives[0].read_bps, STEADY_READ_BPS);
}

#[test]
fn used_pct_is_cached_until_the_refresh_interval() {
    let fake = FakeRoot::new();
    FakeDisk::nvme("nvme0n1", "259:0", XPG_MODEL, XPG_SERIAL)
        .partition("nvme0n1p1", "259:1")
        .write(&fake);
    write_mounts(&fake, &[mount_line("259:1", "/", "ext4", "/dev/nvme0n1p1")]);
    let fs = FakeFs::with_usage(&[("/", ROOT_USED_PCT)]);
    let calls = Arc::clone(&fs.calls);
    let mut disks = DiskCollector::new(&fake.root(), Box::new(fs), &BTreeMap::new());

    let first = disks.sample_after(ONE_SECOND);
    for _ in 1..SAMPLES_PER_REFRESH {
        disks.sample_after(ONE_SECOND);
    }
    let calls_within_window = calls.lock().unwrap().len();
    let cached = disks.sample_after(ONE_SECOND);

    assert_eq!(first.drives[0].used_pct, Some(ROOT_USED_PCT));
    assert_eq!(calls_within_window, 1);
    assert_eq!(calls.lock().unwrap().as_slice(), ["/", "/"]);
    assert_eq!(cached.drives[0].used_pct, Some(ROOT_USED_PCT));
}

#[test]
fn used_pct_is_none_when_statvfs_fails() {
    let fake = FakeRoot::new();
    FakeDisk::nvme("nvme0n1", "259:0", XPG_MODEL, XPG_SERIAL)
        .partition("nvme0n1p1", "259:1")
        .write(&fake);
    write_mounts(&fake, &[mount_line("259:1", "/", "ext4", "/dev/nvme0n1p1")]);

    let snapshot = sample(&fake);

    assert_eq!(snapshot.drives[0].state, DriveState::Live);
    assert_eq!(snapshot.drives[0].used_pct, None);
}

#[test]
fn a_new_mount_is_picked_up_between_rescans() {
    let fake = FakeRoot::new();
    FakeDisk::nvme("nvme0n1", "259:0", XPG_MODEL, XPG_SERIAL)
        .partition("nvme0n1p1", "259:1")
        .write(&fake);
    let fs = FakeFs::with_usage(&[("/mnt/data", ROOT_USED_PCT)]);
    let mut disks = DiskCollector::new(&fake.root(), Box::new(fs), &BTreeMap::new());

    let unmounted = disks.sample_after(ONE_SECOND);
    write_mounts(
        &fake,
        &[mount_line("259:1", "/mnt/data", "ext4", "/dev/nvme0n1p1")],
    );
    let mounted = disks.sample_after(ONE_SECOND);

    assert_eq!(unmounted.drives[0].state, DriveState::Unmounted);
    assert_eq!(unmounted.drives[0].used_pct, None);
    assert_eq!(mounted.drives[0].state, DriveState::Live);
    assert_eq!(mounted.drives[0].mounts, ["/mnt/data"]);
    assert_eq!(mounted.drives[0].used_pct, Some(ROOT_USED_PCT));
}

#[test]
fn a_changed_first_mount_recomputes_used_pct() {
    let fake = FakeRoot::new();
    FakeDisk::nvme("nvme0n1", "259:0", XPG_MODEL, XPG_SERIAL)
        .partition("nvme0n1p1", "259:1")
        .partition("nvme0n1p2", "259:2")
        .write(&fake);
    let data_mount = mount_line("259:1", "/mnt/data", "ext4", "/dev/nvme0n1p1");
    let root_mount = mount_line("259:2", "/", "ext4", "/dev/nvme0n1p2");
    let fs = FakeFs::with_usage(&[("/mnt/data", DATA_USED_PCT), ("/", ROOT_USED_PCT)]);
    let calls = Arc::clone(&fs.calls);
    let mut disks = DiskCollector::new(&fake.root(), Box::new(fs), &BTreeMap::new());
    write_mounts(&fake, std::slice::from_ref(&data_mount));

    let first = disks.sample_after(ONE_SECOND);
    let cached = disks.sample_after(ONE_SECOND);
    let calls_before_change = calls.lock().unwrap().len();
    write_mounts(&fake, &[data_mount, root_mount]);
    let changed = disks.sample_after(ONE_SECOND);

    assert_eq!(first.drives[0].used_pct, Some(DATA_USED_PCT));
    assert_eq!(cached.drives[0].used_pct, Some(DATA_USED_PCT));
    assert_eq!(calls_before_change, 1);
    assert_eq!(changed.drives[0].mounts, ["/", "/mnt/data"]);
    assert_eq!(changed.drives[0].used_pct, Some(ROOT_USED_PCT));
    assert_eq!(calls.lock().unwrap().as_slice(), ["/mnt/data", "/"]);
}

#[test]
fn a_non_utf8_mount_point_does_not_hide_other_mounts() {
    let fake = FakeRoot::new();
    FakeDisk::nvme("nvme0n1", "259:0", XPG_MODEL, XPG_SERIAL)
        .partition("nvme0n1p1", "259:1")
        .write(&fake);
    let invalid_utf8_line: &[u8] = b"36 1 0:50 / /mnt/\xff rw - tmpfs tmpfs rw\n";
    let mut mountinfo = invalid_utf8_line.to_vec();
    mountinfo.extend(mount_line("259:1", "/", "ext4", "/dev/nvme0n1p1").into_bytes());
    fake.write_bytes("proc/self/mountinfo", &mountinfo);

    let snapshot = sample(&fake);

    assert_eq!(snapshot.drives[0].state, DriveState::Live);
    assert_eq!(snapshot.drives[0].mounts, ["/"]);
}

#[test]
fn a_drive_that_dies_between_rescans_turns_dead_on_the_next_sample() {
    let fake = FakeRoot::new();
    FakeDisk::nvme("nvme0n1", "259:0", XPG_MODEL, XPG_SERIAL)
        .partition("nvme0n1p1", "259:1")
        .write(&fake);
    write_mounts(&fake, &[mount_line("259:1", "/", "ext4", "/dev/nvme0n1p1")]);
    let mut disks = collector(&fake);

    let alive = disks.sample_after(ONE_SECOND);
    fake.write("sys/block/nvme0n1/device/state", "dead\n");
    let state_dead = disks.sample_after(ONE_SECOND);
    fake.write("sys/block/nvme0n1/device/state", "live\n");
    let recovered = disks.sample_after(ONE_SECOND);
    fake.write("sys/block/nvme0n1/size", "0\n");
    let size_dead = disks.sample_after(ONE_SECOND);

    assert_eq!(alive.drives[0].state, DriveState::Live);
    assert_eq!(state_dead.drives[0].state, DriveState::Dead);
    assert!(state_dead.drives[0].mounts.is_empty());
    assert_eq!(recovered.drives[0].state, DriveState::Live);
    assert_eq!(size_dead.drives[0].state, DriveState::Dead);
}

#[test]
fn an_ejected_removable_drive_is_unmounted_not_dead() {
    let fake = FakeRoot::new();
    FakeDisk::sata("sdc", "8:32", "USB STICK", Some("USB-1"))
        .removable()
        .partition("sdc1", "8:33")
        .write(&fake);
    write_mounts(
        &fake,
        &[mount_line("8:33", "/run/media/usb", "vfat", "/dev/sdc1")],
    );
    let mut disks = collector(&fake);

    let inserted = disks.sample_after(ONE_SECOND);
    fake.write("sys/block/sdc/size", "0\n");
    write_mounts(&fake, &[]);
    let ejected = disks.sample_after(ONE_SECOND);

    assert_eq!(inserted.drives[0].state, DriveState::Live);
    assert_eq!(ejected.drives[0].state, DriveState::Unmounted);
}

#[test]
fn temps_attach_by_serial() {
    let fake = FakeRoot::new();
    FakeDisk::nvme("nvme0n1", "259:0", KINGSTON_MODEL, KINGSTON_SERIAL).write(&fake);
    FakeDisk::nvme("nvme1n1", "259:5", WD_MODEL, WD_SERIAL).write(&fake);
    FakeDisk::nvme("nvme2n1", "259:7", XPG_MODEL, XPG_SERIAL).write(&fake);
    FakeDisk::sata("sda", "8:0", "WDC WD10EZEX-00W", Some("WD-WMC6Y0L1566R")).write(&fake);
    let mut snapshot = sample(&fake);
    let reading = |id: String, value: Option<f32>| Reading {
        id,
        label: String::new(),
        value,
    };
    let sensors = SensorSnapshot {
        temps: vec![
            reading("k10temp/Tctl".to_owned(), Some(57.9)),
            reading(format!("nvme/{XPG_SERIAL}"), Some(33.85)),
            reading(format!("nvme/{KINGSTON_SERIAL}"), Some(37.85)),
            // The hwmon collector reports a drive it cannot read as a reading without a value.
            reading(format!("nvme/{WD_SERIAL}"), None),
        ],
        fans: Vec::new(),
    };

    attach_temps(&mut snapshot, &sensors);

    assert_eq!(drive(&snapshot, "nvme2n1").temp_c, Some(33.85));
    assert_eq!(drive(&snapshot, "nvme0n1").temp_c, Some(37.85));
    assert_eq!(drive(&snapshot, "nvme1n1").temp_c, None);
    assert_eq!(drive(&snapshot, "sda").temp_c, None);
}

#[test]
fn collector_can_move_to_a_blocking_thread() {
    fn assert_send<T: Send>() {}
    assert_send::<DiskCollector>();
}
