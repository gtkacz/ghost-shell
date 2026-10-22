use std::collections::{BTreeMap, HashMap};
use std::iter;
use std::mem;
use std::time::{Duration, Instant};

use ghost_proto::{DiskSnapshot, Drive, DriveState, SensorSnapshot};
use nix::sys::statvfs::statvfs;

use super::{Source, bytes_per_second, read_text};
use crate::sysroot::SysRoot;

const BLOCK_DIR: &str = "sys/block";
const MOUNTINFO: &str = "proc/self/mountinfo";
const DISKSTATS: &str = "proc/diskstats";
const NOT_A_DRIVE_PREFIXES: [&str; 4] = ["loop", "zram", "ram", "dm-"];

const DEV_FILE: &str = "dev";
const SIZE_FILE: &str = "size";
const DEVICE_DIR: &str = "device";
const MODEL_FILE: &str = "model";
const SERIAL_FILE: &str = "serial";
const WWID_FILE: &str = "wwid";
const STATE_FILE: &str = "state";
const DEAD_STATE: &str = "dead";
const REMOVABLE_FILE: &str = "removable";
const REMOVABLE_FLAG: &str = "1";
const DEVICE_NODE_DIR: &str = "/dev";

const ROOT_MOUNT: &str = "/";
const ID_SEPARATOR: char = '#';
// The hwmon collector names an NVMe drive's temperature `nvme/{serial}`.
const NVME_TEMP_PREFIX: &str = "nvme/";

// The usage cache and the device list both live this long, so a drive that appears or vanishes is
// noticed within a minute while ordinary ticks skip the `sys/block` walk and the statvfs calls.
const REFRESH_INTERVAL_SAMPLES: u32 = 60;

// 1-based positions in /proc/diskstats, counting the major number as field 1.
const FIRST_FIELD_NUMBER: usize = 1;
const NAME_FIELD: usize = 3;
const SECTORS_READ_FIELD: usize = 6;
const SECTORS_WRITTEN_FIELD: usize = 10;
// Sectors in diskstats are 512 bytes whatever the device's own block size.
const BYTES_PER_SECTOR: u64 = 512;

// The mountinfo separator sits between the variable-length optional fields and the filesystem type.
const MOUNTINFO_SEPARATOR: &str = " - ";
const OCTAL_ESCAPE_MARKER: u8 = b'\\';
const OCTAL_ESCAPE_LEN: usize = 4;
const OCTAL_RADIX: u32 = 8;

const FULL_SHARE_PCT: f64 = 100.0;

/// Filesystem usage of a mount point.
///
/// `Send` because collectors are sampled on a blocking thread.
pub trait FsStat: Send {
    /// `None` when the mount cannot be queried or reports no capacity.
    fn used_pct(&self, mount: &str) -> Option<f32>;
}

/// Reads usage with `statvfs(3)`.
pub struct StatvfsFs;

impl FsStat for StatvfsFs {
    fn used_pct(&self, mount: &str) -> Option<f32> {
        let stat = statvfs(mount).ok()?;
        used_percent(stat.blocks(), stat.blocks_free(), stat.blocks_available())
    }
}

/// Matches `df`: root-reserved blocks count as neither used nor available, so a full disk reads 100.
fn used_percent(blocks: u64, blocks_free: u64, blocks_available: u64) -> Option<f32> {
    let used = blocks.saturating_sub(blocks_free);
    let capacity = used.saturating_add(blocks_available);
    (capacity > 0).then(|| (FULL_SHARE_PCT * used as f64 / capacity as f64) as f32)
}

#[derive(Debug, Clone, Copy)]
struct Sectors {
    read: u64,
    written: u64,
}

struct Usage {
    mount: String,
    used_pct: Option<f32>,
}

struct Device {
    kernel_name: String,
    model: String,
    id: String,
    label: String,
    is_removable: bool,
    state_path: String,
    size_path: String,
    device_numbers: Vec<String>,
    device_nodes: Vec<String>,
    usage: Option<Usage>,
    previous_sectors: Option<Sectors>,
}

impl Device {
    // btrfs reports an anonymous device number for its mounts, so only the source ties them to a drive.
    fn owns(&self, mount: &MountEntry) -> bool {
        self.device_numbers
            .iter()
            .any(|number| number == mount.device_number)
            || self.device_nodes.iter().any(|node| node == mount.source)
    }

    /// Read on every sample, so a drive that dies is reported at once rather than at the next rescan.
    fn is_dead(&self, root: &SysRoot) -> bool {
        read_text(root, &self.state_path).as_deref() == Some(DEAD_STATE)
            // A removable drive's size is 0 whenever its medium is out, which is no failure.
            || (!self.is_removable && read_size(root, &self.size_path) == Some(0))
    }

    fn sample(
        &mut self,
        root: &SysRoot,
        mounts: &[MountEntry],
        sectors: Option<Sectors>,
        elapsed: Duration,
        fs: &dyn FsStat,
    ) -> Drive {
        let mut drive = Drive {
            id: self.id.clone(),
            label: self.label.clone(),
            model: self.model.clone(),
            kernel_name: self.kernel_name.clone(),
            state: DriveState::Dead,
            ..Drive::default()
        };
        if self.is_dead(root) {
            // A drive that comes back must not be measured against counters from before it died.
            self.previous_sectors = None;
            return drive;
        }

        // `/` is a prefix of every other absolute mount point, so it sorts first.
        let mut mount_points: Vec<String> = mounts
            .iter()
            .filter(|mount| self.owns(mount))
            .map(|mount| unescape(mount.mount_point))
            .collect();
        mount_points.sort();
        mount_points.dedup();

        drive.state = if mount_points.is_empty() {
            DriveState::Unmounted
        } else {
            DriveState::Live
        };
        drive.used_pct = mount_points
            .first()
            .and_then(|mount| self.used_pct(mount, fs));
        (drive.read_bps, drive.write_bps) = self.rates(sectors, elapsed);
        drive.mounts = mount_points;
        drive
    }

    fn used_pct(&mut self, mount: &str, fs: &dyn FsStat) -> Option<f32> {
        if let Some(cached) = self.usage.as_ref().filter(|usage| usage.mount == mount) {
            return cached.used_pct;
        }
        let used_pct = fs.used_pct(mount);
        self.usage = Some(Usage {
            mount: mount.to_owned(),
            used_pct,
        });
        used_pct
    }

    fn rates(&mut self, current: Option<Sectors>, elapsed: Duration) -> (u64, u64) {
        match (mem::replace(&mut self.previous_sectors, current), current) {
            (Some(before), Some(now)) => (
                sector_bytes_per_second(now.read, before.read, elapsed),
                sector_bytes_per_second(now.written, before.written, elapsed),
            ),
            _ => (0, 0),
        }
    }
}

/// Scales the difference rather than each counter, so large counters cannot saturate it.
fn sector_bytes_per_second(now: u64, before: u64, elapsed: Duration) -> u64 {
    let sectors = now.saturating_sub(before);
    bytes_per_second(sectors.saturating_mul(BYTES_PER_SECTOR), 0, elapsed)
}

/// Physical drives from `/sys/block`, keyed by model and serial rather than by kernel name.
///
/// The kernel numbers NVMe drives in probe order, which can change between boots, so a drive's
/// `id` is `{model}#{serial}`. The device list and each drive's usage are rebuilt every
/// `REFRESH_INTERVAL_SAMPLES` samples; ordinary ticks read `mountinfo`, `diskstats` and each known
/// drive's `state` and `size`.
pub struct DiskCollector {
    root: SysRoot,
    fs: Box<dyn FsStat + Send>,
    labels: BTreeMap<String, String>,
    devices: Vec<Device>,
    samples_since_refresh: u32,
    last_sampled: Option<Instant>,
    mountinfo: Vec<u8>,
}

impl DiskCollector {
    /// `labels` maps a drive serial to the label that replaces its model.
    pub fn new(
        root: &SysRoot,
        fs: Box<dyn FsStat + Send>,
        labels: &BTreeMap<String, String>,
    ) -> Self {
        Self {
            root: root.clone(),
            fs,
            labels: labels.clone(),
            devices: Vec::new(),
            // Due immediately, so the first sample enumerates the drives and reads their usage.
            samples_since_refresh: REFRESH_INTERVAL_SAMPLES,
            last_sampled: None,
            mountinfo: Vec::new(),
        }
    }

    /// Samples with the time since the previous sample supplied by the caller.
    pub fn sample_after(&mut self, elapsed: Duration) -> DiskSnapshot {
        if self.samples_since_refresh >= REFRESH_INTERVAL_SAMPLES {
            self.rescan();
            self.samples_since_refresh = 0;
        }
        self.samples_since_refresh += 1;

        if self.root.read_into(MOUNTINFO, &mut self.mountinfo).is_err() {
            self.mountinfo.clear();
        }
        // One undecodable byte in a mount point must not hide every other mount.
        let mountinfo = String::from_utf8_lossy(&self.mountinfo);
        let mounts: Vec<MountEntry> = mountinfo.lines().filter_map(parse_mount).collect();
        let diskstats = self.root.read(DISKSTATS).unwrap_or_default();
        let sectors: HashMap<&str, Sectors> = parse_diskstats(&diskstats).collect();

        let mut drives: Vec<Drive> = self
            .devices
            .iter_mut()
            .map(|device| {
                let device_sectors = sectors.get(device.kernel_name.as_str()).copied();
                device.sample(
                    &self.root,
                    &mounts,
                    device_sectors,
                    elapsed,
                    self.fs.as_ref(),
                )
            })
            .collect();
        drives.sort_by(|a, b| {
            placement(a)
                .cmp(&placement(b))
                .then_with(|| a.id.cmp(&b.id))
        });
        DiskSnapshot { drives }
    }

    fn rescan(&mut self) {
        // A transient sysfs failure would otherwise drop every drive for a whole window.
        let Some(mut devices) = self.scan() else {
            return;
        };
        for device in &mut devices {
            device.previous_sectors = self
                .devices
                .iter()
                .find(|old| old.kernel_name == device.kernel_name && old.id == device.id)
                .and_then(|old| old.previous_sectors);
        }
        self.devices = devices;
    }

    fn scan(&self) -> Option<Vec<Device>> {
        let names = self.root.list(BLOCK_DIR).ok()?;
        Some(names.iter().filter_map(|name| self.device(name)).collect())
    }

    /// `None` for anything that is not a drive worth listing.
    fn device(&self, kernel_name: &str) -> Option<Device> {
        if NOT_A_DRIVE_PREFIXES
            .iter()
            .any(|prefix| kernel_name.starts_with(prefix))
        {
            return None;
        }
        let dir = format!("{BLOCK_DIR}/{kernel_name}");
        // Loop, md, nbd and the like have no hardware behind them, which sysfs shows as no `device`.
        if !self.root.path(&format!("{dir}/{DEVICE_DIR}")).is_dir() {
            return None;
        }
        let size_path = format!("{dir}/{SIZE_FILE}");
        let is_removable = read_text(&self.root, &format!("{dir}/{REMOVABLE_FILE}")).as_deref()
            == Some(REMOVABLE_FLAG);
        // An empty DVD drive or card reader is neither failing nor holding anything.
        if is_removable && read_size(&self.root, &size_path) == Some(0) {
            return None;
        }
        let read_device_file =
            |file: &str| read_text(&self.root, &format!("{dir}/{DEVICE_DIR}/{file}"));
        let model = read_device_file(MODEL_FILE).unwrap_or_else(|| kernel_name.to_owned());
        let serial = read_device_file(SERIAL_FILE)
            .or_else(|| read_device_file(WWID_FILE))
            .unwrap_or_else(|| kernel_name.to_owned());

        // A partition's name starts with its disk's; the other entries (`queue`, `power`, ...) do not.
        let partitions: Vec<String> = self
            .root
            .list(&dir)
            .unwrap_or_default()
            .into_iter()
            .filter(|entry| entry.starts_with(kernel_name))
            .collect();
        let device_numbers = iter::once(dir.clone())
            .chain(
                partitions
                    .iter()
                    .map(|partition| format!("{dir}/{partition}")),
            )
            .filter_map(|member_dir| read_text(&self.root, &format!("{member_dir}/{DEV_FILE}")))
            .collect();
        let device_nodes = iter::once(kernel_name)
            .chain(partitions.iter().map(String::as_str))
            .map(|member| format!("{DEVICE_NODE_DIR}/{member}"))
            .collect();

        Some(Device {
            kernel_name: kernel_name.to_owned(),
            id: format!("{model}{ID_SEPARATOR}{serial}"),
            label: self
                .labels
                .get(&serial)
                .cloned()
                .unwrap_or_else(|| model.clone()),
            model,
            is_removable,
            state_path: format!("{dir}/{DEVICE_DIR}/{STATE_FILE}"),
            size_path,
            device_numbers,
            device_nodes,
            usage: None,
            previous_sectors: None,
        })
    }
}

impl Source<DiskSnapshot> for DiskCollector {
    fn sample(&mut self) -> DiskSnapshot {
        let now = Instant::now();
        let elapsed = self
            .last_sampled
            .replace(now)
            .map_or(Duration::ZERO, |before| now.duration_since(before));
        self.sample_after(elapsed)
    }

    fn resume(&mut self) {
        // Usage is cached and the drive list kept until the next refresh, so it is made due now.
        self.samples_since_refresh = REFRESH_INTERVAL_SAMPLES;
    }
}

/// Sets each drive's temperature from the `nvme/{serial}` reading the sensor collector made for it.
pub fn attach_temps(disks: &mut DiskSnapshot, sensors: &SensorSnapshot) {
    for drive in &mut disks.drives {
        let Some(serial) = serial_of(drive) else {
            continue;
        };
        if let Some(reading) = sensors
            .temps
            .iter()
            .find(|reading| reading.id.strip_prefix(NVME_TEMP_PREFIX) == Some(serial))
        {
            drive.temp_c = reading.value;
        }
    }
}

/// `Drive` carries no serial of its own, so it is read back out of the id. The stored model is
/// stripped rather than the id split at a `#`, because either part may contain one itself.
fn serial_of(drive: &Drive) -> Option<&str> {
    drive
        .id
        .strip_prefix(drive.model.as_str())?
        .strip_prefix(ID_SEPARATOR)
}

// Variants are declared in display order, which `Ord` then follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Placement {
    Root,
    Live,
    Dead,
    Unmounted,
}

fn placement(drive: &Drive) -> Placement {
    match drive.state {
        DriveState::Live if drive.mounts.first().map(String::as_str) == Some(ROOT_MOUNT) => {
            Placement::Root
        }
        DriveState::Live => Placement::Live,
        DriveState::Dead => Placement::Dead,
        DriveState::Unmounted => Placement::Unmounted,
    }
}

/// One `/proc/self/mountinfo` line, with the fields left as the kernel escaped them.
struct MountEntry<'a> {
    device_number: &'a str,
    mount_point: &'a str,
    source: &'a str,
}

fn parse_mount(line: &str) -> Option<MountEntry<'_>> {
    let (head, tail) = line.split_once(MOUNTINFO_SEPARATOR)?;
    let mut head = head.split_ascii_whitespace();
    let (_mount_id, _parent_id, device_number, _root, mount_point) = (
        head.next()?,
        head.next()?,
        head.next()?,
        head.next()?,
        head.next()?,
    );
    let mut tail = tail.split_ascii_whitespace();
    let (_fs_type, source) = (tail.next()?, tail.next()?);
    Some(MountEntry {
        device_number,
        mount_point,
        source,
    })
}

/// The kernel writes whitespace and backslashes in a mount point as `\ooo` octal escapes.
fn unescape(raw: &str) -> String {
    let mut decoded = Vec::with_capacity(raw.len());
    let mut rest = raw.as_bytes();
    while let Some((&byte, tail)) = rest.split_first() {
        if let Some(escaped) = octal_escape(rest) {
            decoded.push(escaped);
            rest = &rest[OCTAL_ESCAPE_LEN..];
        } else {
            decoded.push(byte);
            rest = tail;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn octal_escape(bytes: &[u8]) -> Option<u8> {
    let [OCTAL_ESCAPE_MARKER, digits @ ..] = bytes else {
        return None;
    };
    let digits = digits.get(..OCTAL_ESCAPE_LEN - 1)?;
    let value = digits.iter().try_fold(0u32, |value, digit| {
        Some(value * OCTAL_RADIX + char::from(*digit).to_digit(OCTAL_RADIX)?)
    })?;
    u8::try_from(value).ok()
}

fn parse_diskstats(text: &str) -> impl Iterator<Item = (&str, Sectors)> {
    text.lines().filter_map(|line| {
        let mut name: Option<&str> = None;
        let mut read: Option<u64> = None;
        let mut written: Option<u64> = None;
        for (index, token) in line.split_ascii_whitespace().enumerate() {
            match FIRST_FIELD_NUMBER + index {
                NAME_FIELD => name = Some(token),
                SECTORS_READ_FIELD => read = token.parse().ok(),
                SECTORS_WRITTEN_FIELD => {
                    written = token.parse().ok();
                    break;
                }
                _ => {}
            }
        }
        Some((
            name?,
            Sectors {
                read: read?,
                written: written?,
            },
        ))
    })
}

fn read_size(root: &SysRoot, rel: &str) -> Option<u64> {
    read_text(root, rel)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn used_percent_excludes_reserved_blocks() {
        // 1000 blocks of which 600 are used, 300 are available and 100 are reserved for root.
        let used = used_percent(1000, 400, 300).unwrap();

        assert!((used - 66.67).abs() < 0.01);
    }

    #[test]
    fn used_percent_is_none_without_capacity() {
        assert_eq!(used_percent(0, 0, 0), None);
    }

    #[test]
    fn used_percent_tolerates_free_blocks_above_total() {
        assert_eq!(used_percent(100, 200, 50), Some(0.0));
    }

    #[test]
    fn unescape_decodes_octal_and_keeps_stray_backslashes() {
        assert_eq!(unescape(r"/mnt/My\040Disk\134x"), r"/mnt/My Disk\x");
        assert_eq!(unescape(r"/mnt/a\0"), r"/mnt/a\0");
        assert_eq!(unescape(r"/mnt/a\777"), r"/mnt/a\777");
        assert_eq!(unescape(r"/mnt/\"), r"/mnt/\");
    }
}
