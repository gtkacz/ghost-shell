mod common;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use common::FakeRoot;
use common::disk::*;
use ghost_proto::DriveState;
use ghostd::collect::DiskCollector;

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
fn collector_can_move_to_a_blocking_thread() {
    fn assert_send<T: Send>() {}
    assert_send::<DiskCollector>();
}
