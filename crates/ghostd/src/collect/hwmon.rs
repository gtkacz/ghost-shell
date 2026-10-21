use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use ghost_proto::{Reading, SensorSnapshot};

use super::Source;
use super::labels;
use crate::sysroot::SysRoot;

const HWMON_DIR: &str = "sys/class/hwmon";
const HWMON_ENTRY_PREFIX: &str = "hwmon";
const CHIP_NAME_FILE: &str = "name";
const DEVICE_LINK: &str = "device";
const SERIAL_FILE: &str = "serial";
const MODEL_FILE: &str = "model";
const INPUT_SUFFIX: &str = "_input";
const LABEL_SUFFIX: &str = "_label";

const NVME_CHIP: &str = "nvme";
const SPD5118_CHIP: &str = "spd5118";
// An NVMe controller's further temperatures would share its serial-based id, and a DIMM has no others.
const SINGLE_READING_TEMP: u32 = 1;
const FIRST_DIMM_POSITION: usize = 1;

const RESCAN_INTERVAL_SAMPLES: u32 = 30;
const MILLIDEGREES_PER_DEGREE: f32 = 1000.0;

// Temps sort before fans, which keeps each kind in input order once they are split apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Temp,
    Fan,
}

impl Kind {
    const ALL: [Kind; 2] = [Kind::Temp, Kind::Fan];

    fn stem(self) -> &'static str {
        match self {
            Kind::Temp => "temp",
            Kind::Fan => "fan",
        }
    }
}

struct Chip {
    dir: String,
    name: String,
}

struct Input {
    kind: Kind,
    number: u32,
    // The `tempN`/`fanN` file stem, kept verbatim since it names both the value and label files.
    stem: String,
}

enum Naming {
    Drive {
        serial: String,
        model: Option<String>,
    },
    Dimm {
        position: usize,
    },
    Generic,
}

struct Sensor {
    kind: Kind,
    id: String,
    label: String,
    input_path: String,
}

impl Sensor {
    fn read(&self, root: &SysRoot) -> Option<f32> {
        let raw = read_raw(root, &self.input_path)? as f32;
        Some(match self.kind {
            Kind::Temp => raw / MILLIDEGREES_PER_DEGREE,
            Kind::Fan => raw,
        })
    }
}

/// Temperatures and fan speeds from `/sys/class/hwmon`.
///
/// Chips are listed in hwmon index order and each chip's inputs in numeric order. The sensor list
/// is rebuilt every `RESCAN_INTERVAL_SAMPLES` samples, so ordinary ticks only read value files.
pub struct SensorCollector {
    root: SysRoot,
    overrides: BTreeMap<String, String>,
    sensors: Vec<Sensor>,
    samples_since_scan: u32,
}

impl SensorCollector {
    /// `overrides` maps a sensor id such as `k10temp/Tctl` to the label that replaces the built-in.
    pub fn new(root: &SysRoot, overrides: &BTreeMap<String, String>) -> Self {
        Self {
            sensors: Scan::new(root, overrides, HashSet::new()).sensors(),
            root: root.clone(),
            overrides: overrides.clone(),
            samples_since_scan: 0,
        }
    }

    fn rescan(&mut self) {
        // A fan that was listed stays listed even if it has since stopped, which is when it matters.
        let listed_fans = self
            .sensors
            .iter()
            .filter(|sensor| sensor.kind == Kind::Fan)
            .map(|sensor| sensor.id.as_str())
            .collect();
        let rescanned = Scan::new(&self.root, &self.overrides, listed_fans).sensors();
        self.sensors = rescanned;
        self.samples_since_scan = 0;
    }
}

impl Source<SensorSnapshot> for SensorCollector {
    fn sample(&mut self) -> SensorSnapshot {
        if self.samples_since_scan >= RESCAN_INTERVAL_SAMPLES {
            self.rescan();
        }
        self.samples_since_scan += 1;

        let mut snapshot = SensorSnapshot::default();
        for sensor in &self.sensors {
            let reading = Reading {
                id: sensor.id.clone(),
                label: sensor.label.clone(),
                value: sensor.read(&self.root),
            };
            match sensor.kind {
                Kind::Temp => snapshot.temps.push(reading),
                Kind::Fan => snapshot.fans.push(reading),
            }
        }
        snapshot
    }
}

struct Scan<'a> {
    root: &'a SysRoot,
    overrides: &'a BTreeMap<String, String>,
    listed_fans: HashSet<&'a str>,
}

impl<'a> Scan<'a> {
    fn new(
        root: &'a SysRoot,
        overrides: &'a BTreeMap<String, String>,
        listed_fans: HashSet<&'a str>,
    ) -> Self {
        Self {
            root,
            overrides,
            listed_fans,
        }
    }

    fn sensors(&self) -> Vec<Sensor> {
        let chips = self.chips();
        let dimm_positions = self.dimm_positions(&chips);
        chips
            .iter()
            .filter_map(|chip| Some((chip, self.naming(chip, &dimm_positions)?)))
            .flat_map(|(chip, naming)| self.chip_sensors(chip, &naming))
            .collect()
    }

    fn chips(&self) -> Vec<Chip> {
        let Ok(entries) = self.root.list(HWMON_DIR) else {
            return Vec::new();
        };
        let mut indexed: Vec<(u32, &String)> = entries
            .iter()
            .filter_map(|entry| {
                Some((entry.strip_prefix(HWMON_ENTRY_PREFIX)?.parse().ok()?, entry))
            })
            .collect();
        // Entry names sort lexically (hwmon10 before hwmon2), so restore numeric order.
        indexed.sort_unstable();
        indexed
            .into_iter()
            .filter_map(|(_, entry)| {
                let dir = format!("{HWMON_DIR}/{entry}");
                let name = read_text(self.root, &format!("{dir}/{CHIP_NAME_FILE}"))?;
                Some(Chip { dir, name })
            })
            .collect()
    }

    /// 1-based DIMM positions keyed by hwmon dir, ordered by the DIMM's device path.
    fn dimm_positions<'c>(&self, chips: &'c [Chip]) -> HashMap<&'c str, usize> {
        let mut devices: Vec<(PathBuf, &str)> = chips
            .iter()
            .filter(|chip| chip.name == SPD5118_CHIP)
            .filter_map(|chip| {
                let link = self.root.path(&format!("{}/{DEVICE_LINK}", chip.dir));
                Some((fs::canonicalize(link).ok()?, chip.dir.as_str()))
            })
            .collect();
        // hwmonN follows probe order, whereas the i2c address is fixed by the slot the DIMM sits in.
        devices.sort();
        devices
            .into_iter()
            .enumerate()
            .map(|(index, (_, dir))| (dir, FIRST_DIMM_POSITION + index))
            .collect()
    }

    fn naming(&self, chip: &Chip, dimm_positions: &HashMap<&str, usize>) -> Option<Naming> {
        match chip.name.as_str() {
            // Without a serial the id would collide between drives, so the chip is left out.
            NVME_CHIP => Some(Naming::Drive {
                serial: read_text(
                    self.root,
                    &format!("{}/{DEVICE_LINK}/{SERIAL_FILE}", chip.dir),
                )?,
                model: read_text(
                    self.root,
                    &format!("{}/{DEVICE_LINK}/{MODEL_FILE}", chip.dir),
                ),
            }),
            SPD5118_CHIP => dimm_positions
                .get(chip.dir.as_str())
                .map(|&position| Naming::Dimm { position }),
            _ => Some(Naming::Generic),
        }
    }

    fn chip_sensors(&self, chip: &Chip, naming: &Naming) -> Vec<Sensor> {
        let Ok(files) = self.root.list(&chip.dir) else {
            return Vec::new();
        };
        let mut inputs: Vec<Input> = files.iter().filter_map(|file| parse_input(file)).collect();
        inputs.sort_unstable_by_key(|input| (input.kind, input.number));
        inputs
            .iter()
            .filter_map(|input| self.sensor(chip, naming, input))
            .collect()
    }

    fn sensor(&self, chip: &Chip, naming: &Naming, input: &Input) -> Option<Sensor> {
        let input_path = format!("{}/{}{INPUT_SUFFIX}", chip.dir, input.stem);
        let raw_label = read_text(
            self.root,
            &format!("{}/{}{LABEL_SUFFIX}", chip.dir, input.stem),
        );
        let sensor_name = raw_label.as_deref().unwrap_or(&input.stem);
        let (id, builtin_label) = identify(chip, naming, input, sensor_name)?;
        let override_label = self.overrides.get(&id);

        let is_named = override_label.is_some() || builtin_label.is_some();
        // Unplugged fan headers read 0 rpm forever, so an unnamed fan has to prove it is wired up.
        if input.kind == Kind::Fan
            && !is_named
            && !self.listed_fans.contains(id.as_str())
            && !read_raw(self.root, &input_path).is_some_and(|rpm| rpm > 0)
        {
            return None;
        }

        let label = override_label
            .cloned()
            .or(builtin_label)
            .unwrap_or_else(|| labels::fallback_label(&chip.name, sensor_name));
        Some(Sensor {
            kind: input.kind,
            id,
            label,
            input_path,
        })
    }
}

/// The sensor's id and built-in label; `None` drops inputs that would only duplicate an id.
fn identify(
    chip: &Chip,
    naming: &Naming,
    input: &Input,
    sensor_name: &str,
) -> Option<(String, Option<String>)> {
    let is_single_reading = input.kind == Kind::Temp && input.number == SINGLE_READING_TEMP;
    match naming {
        Naming::Drive { serial, model } => is_single_reading.then(|| {
            (
                format!("{}/{serial}", chip.name),
                model.as_deref().map(labels::drive_label),
            )
        }),
        Naming::Dimm { position } => is_single_reading.then(|| {
            (
                format!("{}/{position}", chip.name),
                Some(labels::dimm_label(*position)),
            )
        }),
        Naming::Generic => Some((
            format!("{}/{sensor_name}", chip.name),
            labels::known_label(&chip.name, sensor_name).map(str::to_owned),
        )),
    }
}

fn parse_input(file: &str) -> Option<Input> {
    let stem = file.strip_suffix(INPUT_SUFFIX)?;
    Kind::ALL.into_iter().find_map(|kind| {
        Some(Input {
            kind,
            number: stem.strip_prefix(kind.stem())?.parse().ok()?,
            stem: stem.to_owned(),
        })
    })
}

/// Trimmed file content, or `None` when the file is unreadable or blank.
fn read_text(root: &SysRoot, rel: &str) -> Option<String> {
    let text = root.read(rel).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Parsed as an integer, which sysfs always reports, so a garbled `nan` or `inf` cannot get through.
fn read_raw(root: &SysRoot, rel: &str) -> Option<i64> {
    root.read(rel).ok()?.trim().parse().ok()
}
