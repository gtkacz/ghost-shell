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
const FIRST_ORDINAL: usize = 1;

const RESCAN_INTERVAL_SAMPLES: u32 = 30;
// The nct6775 driver refreshes its whole register bank on any read once its 1.5 s cache is stale
// (~6 ms of CPU), so any cadence at or above the cache lifetime pays a full refresh per read.
const SUPERIO_READ_EVERY: u32 = 5;
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
    ordinal: usize,
}

impl Chip {
    // Repeated chip names would share ids; the first keeps the bare name so lone chips keep theirs.
    fn reference(&self) -> String {
        if self.ordinal == FIRST_ORDINAL {
            self.name.clone()
        } else {
            format!("{}#{}", self.name, self.ordinal)
        }
    }
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
    Dimm,
    Generic,
}

struct Sensor {
    kind: Kind,
    id: String,
    label: String,
    input_path: String,
    is_super_io: bool,
    value: Option<f32>,
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
/// Super-I/O chips (`nct*`, `it8*`) are read every `SUPERIO_READ_EVERY` samples, repeating their
/// last values in between, and list only temperatures that carry a built-in or override label.
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
        // A transient sysfs failure would otherwise blank every row for a whole window.
        if !rescanned.is_empty() {
            self.sensors = rescanned;
        }
        self.samples_since_scan = 0;
    }
}

impl Source<SensorSnapshot> for SensorCollector {
    fn sample(&mut self) -> SensorSnapshot {
        if self.samples_since_scan >= RESCAN_INTERVAL_SAMPLES {
            self.rescan();
        }
        let reads_super_io = self.samples_since_scan.is_multiple_of(SUPERIO_READ_EVERY);
        self.samples_since_scan += 1;

        let mut snapshot = SensorSnapshot::default();
        for sensor in &mut self.sensors {
            if reads_super_io || !sensor.is_super_io {
                sensor.value = sensor.read(&self.root);
            }
            let reading = Reading {
                id: sensor.id.clone(),
                label: sensor.label.clone(),
                value: sensor.value,
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
        self.chips()
            .iter()
            .filter_map(|chip| Some((chip, self.naming(chip)?)))
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
        let named: Vec<(String, String)> = indexed
            .into_iter()
            .filter_map(|(_, entry)| {
                let dir = format!("{HWMON_DIR}/{entry}");
                let name = read_text(self.root, &format!("{dir}/{CHIP_NAME_FILE}"))?;
                Some((dir, name))
            })
            .collect();
        let ordinals = self.ordinals(&named);
        named
            .into_iter()
            .filter_map(|(dir, name)| {
                let ordinal = *ordinals.get(&dir)?;
                Some(Chip { dir, name, ordinal })
            })
            .collect()
    }

    fn ordinals(&self, named: &[(String, String)]) -> HashMap<String, usize> {
        let mut by_name: BTreeMap<&str, Vec<(PathBuf, &str)>> = BTreeMap::new();
        for (dir, name) in named {
            if let Some(device) = self.device_path(dir) {
                by_name
                    .entry(name.as_str())
                    .or_default()
                    .push((device, dir.as_str()));
            }
        }
        by_name
            .into_values()
            .flat_map(|mut group| {
                // hwmonN follows probe order; the device path is fixed by where the hardware sits.
                group.sort();
                group
                    .into_iter()
                    .enumerate()
                    .map(|(index, (_, dir))| (dir.to_owned(), FIRST_ORDINAL + index))
            })
            .collect()
    }

    fn device_path(&self, dir: &str) -> Option<PathBuf> {
        // Hwmon devices registered without a parent have no `device` link, but their own path
        // still orders them.
        fs::canonicalize(self.root.path(&format!("{dir}/{DEVICE_LINK}")))
            .or_else(|_| fs::canonicalize(self.root.path(dir)))
            .ok()
    }

    fn naming(&self, chip: &Chip) -> Option<Naming> {
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
            SPD5118_CHIP => Some(Naming::Dimm),
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
        if !is_named && !self.is_worth_listing(chip, input, &id, &input_path) {
            return None;
        }

        let label = override_label
            .cloned()
            .or(builtin_label)
            .unwrap_or_else(|| labels::fallback_label(&chip.reference(), sensor_name));
        Some(Sensor {
            kind: input.kind,
            id,
            label,
            input_path,
            is_super_io: labels::is_super_io(&chip.name),
            value: None,
        })
    }

    fn is_worth_listing(&self, chip: &Chip, input: &Input, id: &str, input_path: &str) -> bool {
        match input.kind {
            // Super-I/O chips expose a dozen virtual or unwired temps beside the few real ones.
            Kind::Temp => !labels::is_super_io(&chip.name),
            // Unplugged fan headers read 0 rpm forever, so an unnamed fan must prove it is wired.
            Kind::Fan => {
                self.listed_fans.contains(id)
                    || read_raw(self.root, input_path).is_some_and(|rpm| rpm > 0)
            }
        }
    }
}

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
        Naming::Dimm => is_single_reading.then(|| {
            (
                format!("{}/{}", chip.name, chip.ordinal),
                Some(labels::dimm_label(chip.ordinal)),
            )
        }),
        Naming::Generic => Some((
            format!("{}/{sensor_name}", chip.reference()),
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

fn read_text(root: &SysRoot, rel: &str) -> Option<String> {
    let text = root.read(rel).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Parsed as an integer, which sysfs always reports, so a garbled `nan` or `inf` cannot get through.
fn read_raw(root: &SysRoot, rel: &str) -> Option<i64> {
    root.read(rel).ok()?.trim().parse().ok()
}
