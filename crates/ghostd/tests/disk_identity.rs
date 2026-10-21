mod common;

use std::collections::BTreeMap;

use common::FakeRoot;
use common::disk::*;
use ghost_proto::{DriveState, Reading, SensorSnapshot};
use ghostd::collect::{DiskCollector, attach_temps};

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
fn temps_attach_when_the_serial_or_the_model_contains_the_id_separator() {
    let fake = FakeRoot::new();
    FakeDisk::nvme("nvme0n1", "259:0", KINGSTON_MODEL, "AB#CD").write(&fake);
    FakeDisk::nvme("nvme1n1", "259:5", "MODEL #2", WD_SERIAL).write(&fake);
    let mut snapshot = sample(&fake);
    let reading = |serial: &str, value: f32| Reading {
        id: format!("nvme/{serial}"),
        label: String::new(),
        value: Some(value),
    };
    let sensors = SensorSnapshot {
        temps: vec![reading("AB#CD", 41.5), reading(WD_SERIAL, 38.25)],
        fans: Vec::new(),
    };

    attach_temps(&mut snapshot, &sensors);

    assert_eq!(drive(&snapshot, "nvme0n1").temp_c, Some(41.5));
    assert_eq!(drive(&snapshot, "nvme1n1").temp_c, Some(38.25));
}
