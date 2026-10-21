mod common;

use std::collections::BTreeMap;

use common::FakeRoot;
use ghost_proto::{Reading, SensorSnapshot};
use ghostd::collect::{SensorCollector, Source};

const HWMON_DIR: &str = "sys/class/hwmon";
const NVME_DIR: &str = "sys/class/nvme";
const I2C_DIR: &str = "sys/devices/pci0000:00/0000:00:14.0/i2c-2";
const RESCAN_EVERY: usize = 30;
const SUPERIO_READ_EVERY: usize = 5;
const SERIAL_WIDTH: usize = 20;
const MODEL_WIDTH: usize = 40;

const KINGSTON_SERIAL: &str = "50026B7684E5C533";
const KINGSTON_MODEL: &str = "KINGSTON SKC2500M8250G";
const WD_SERIAL: &str = "213823802844";
const WD_MODEL: &str = "WDC WDS960G2G0C-00AJM0";
const XPG_SERIAL: &str = "2O412LA45EG7";
const XPG_MODEL: &str = "XPG GAMMIX S70 BLADE";

fn chip_dir(index: u32) -> String {
    format!("{HWMON_DIR}/hwmon{index}")
}

fn write_chip(fake: &FakeRoot, index: u32, name: &str, files: &[(&str, &str)]) {
    let dir = chip_dir(index);
    fake.write(&format!("{dir}/name"), &format!("{name}\n"));
    for (file, content) in files {
        fake.write(&format!("{dir}/{file}"), &format!("{content}\n"));
    }
}

/// Sysfs pads `serial` and `model` with spaces, so the fake does too.
fn write_nvme(
    fake: &FakeRoot,
    hwmon_index: u32,
    nvme_index: u32,
    serial: &str,
    model: &str,
    temps: &[(&str, &str)],
) {
    let controller = format!("{NVME_DIR}/nvme{nvme_index}");
    fake.write(
        &format!("{controller}/serial"),
        &format!("{serial:<SERIAL_WIDTH$}\n"),
    );
    fake.write(
        &format!("{controller}/model"),
        &format!("{model:<MODEL_WIDTH$}\n"),
    );
    write_chip(fake, hwmon_index, "nvme", &[]);
    for (position, (label, millidegrees)) in temps.iter().enumerate() {
        let number = position + 1;
        let dir = chip_dir(hwmon_index);
        fake.write(&format!("{dir}/temp{number}_label"), &format!("{label}\n"));
        fake.write(
            &format!("{dir}/temp{number}_input"),
            &format!("{millidegrees}\n"),
        );
    }
    fake.link_dir(&format!("{}/device", chip_dir(hwmon_index)), &controller);
}

fn write_dimm(fake: &FakeRoot, hwmon_index: u32, i2c_address: &str, millidegrees: &str) {
    write_chip(
        fake,
        hwmon_index,
        "spd5118",
        &[("temp1_input", millidegrees)],
    );
    fake.link_dir(
        &format!("{}/device", chip_dir(hwmon_index)),
        &format!("{I2C_DIR}/{i2c_address}"),
    );
}

/// The real box: hwmon numbering, fans and chips as `/sys/class/hwmon` shows them.
fn machine() -> FakeRoot {
    let fake = FakeRoot::new();
    write_chip(
        &fake,
        0,
        "nct6799",
        &[
            ("temp1_label", "SYSTIN"),
            ("temp1_input", "36000"),
            ("temp2_input", "34000"),
            ("temp3_label", "CPUTIN"),
            ("temp3_input", "35000"),
            ("fan1_input", "0"),
            ("fan2_input", "1670"),
            ("fan3_input", "450"),
            ("fan4_input", "0"),
        ],
    );
    write_nvme(
        &fake,
        1,
        0,
        KINGSTON_SERIAL,
        KINGSTON_MODEL,
        &[("Composite", "37850")],
    );
    write_nvme(&fake, 2, 1, WD_SERIAL, WD_MODEL, &[("Composite", "0")]);
    // The dead drive's temperature read fails with EIO, which a directory reproduces portably.
    let dead_input = format!("{}/temp1_input", chip_dir(2));
    fake.remove(&dead_input);
    fake.write(&format!("{dead_input}/unreadable"), "");
    write_nvme(
        &fake,
        3,
        2,
        XPG_SERIAL,
        XPG_MODEL,
        &[
            ("Composite", "33850"),
            ("Sensor 1", "33850"),
            ("Sensor 2", "35850"),
        ],
    );
    write_chip(&fake, 4, "ps_controller_battery_14:3a:9a:fc:ea:23", &[]);
    write_chip(
        &fake,
        5,
        "k10temp",
        &[
            ("temp1_label", "Tctl"),
            ("temp1_input", "57875"),
            ("temp3_label", "Tccd1"),
            ("temp3_input", "61625"),
            ("temp4_label", "Tccd2"),
            ("temp4_input", "34625"),
        ],
    );
    write_chip(
        &fake,
        6,
        "r8169_0_e00:00",
        &[("temp1_input", "41500"), ("temp1_max", "120000")],
    );
    write_dimm(&fake, 7, "2-0051", "31500");
    write_dimm(&fake, 8, "2-0053", "32250");
    fake
}

fn reading(id: &str, label: &str, value: Option<f32>) -> Reading {
    Reading {
        id: id.to_owned(),
        label: label.to_owned(),
        value,
    }
}

fn collector(fake: &FakeRoot) -> SensorCollector {
    SensorCollector::new(&fake.root(), &BTreeMap::new())
}

fn temp<'a>(snapshot: &'a SensorSnapshot, id: &str) -> &'a Reading {
    snapshot
        .temps
        .iter()
        .find(|reading| reading.id == id)
        .unwrap_or_else(|| panic!("no temperature with id {id}"))
}

fn fan_values(snapshot: &SensorSnapshot) -> Vec<Option<f32>> {
    snapshot.fans.iter().map(|fan| fan.value).collect()
}

fn sample_through_rescan(sensors: &mut SensorCollector) -> SensorSnapshot {
    for _ in 0..RESCAN_EVERY {
        sensors.sample();
    }
    sensors.sample()
}

#[test]
fn labels_and_values() {
    let fake = machine();

    let snapshot = collector(&fake).sample();

    assert_eq!(
        snapshot.temps,
        vec![
            reading("nct6799/SYSTIN", "MOBO", Some(36.0)),
            reading(
                &format!("nvme/{KINGSTON_SERIAL}"),
                KINGSTON_MODEL,
                Some(37.85)
            ),
            reading(&format!("nvme/{WD_SERIAL}"), WD_MODEL, None),
            reading(&format!("nvme/{XPG_SERIAL}"), XPG_MODEL, Some(33.85)),
            reading("k10temp/Tctl", "CPU TCTL", Some(57.875)),
            reading("k10temp/Tccd1", "CCD0", Some(61.625)),
            reading("k10temp/Tccd2", "CCD1", Some(34.625)),
            reading("r8169_0_e00:00/temp1", "NIC", Some(41.5)),
            reading("spd5118/1", "DIMM 1", Some(31.5)),
            reading("spd5118/2", "DIMM 2", Some(32.25)),
        ]
    );
    assert_eq!(
        snapshot.fans,
        vec![
            reading("nct6799/fan2", "CPU FAN", Some(1670.0)),
            reading("nct6799/fan3", "CHASSIS FAN", Some(450.0)),
        ]
    );
}

#[test]
fn nvme_uses_serial_and_model() {
    let fake = machine();

    let snapshot = collector(&fake).sample();

    let xpg = temp(&snapshot, "nvme/2O412LA45EG7");
    assert_eq!(xpg.label, "XPG GAMMIX S70 BLADE");
    assert_eq!(xpg.value, Some(33.85));
    let nvme_readings = snapshot
        .temps
        .iter()
        .filter(|reading| reading.id.starts_with("nvme/"))
        .count();
    assert_eq!(nvme_readings, 3, "only each drive's composite is reported");
}

#[test]
fn failed_read_is_unavailable_not_fatal() {
    let fake = machine();
    fake.write(&format!("{}/temp1_input", chip_dir(0)), "not a number\n");

    let snapshot = collector(&fake).sample();

    assert_eq!(temp(&snapshot, &format!("nvme/{WD_SERIAL}")).value, None);
    assert_eq!(temp(&snapshot, "nct6799/SYSTIN").value, None);
    assert_eq!(temp(&snapshot, "k10temp/Tctl").value, Some(57.875));
    assert_eq!(temp(&snapshot, "nct6799/SYSTIN").label, "MOBO");
    assert_eq!(fan_values(&snapshot), [Some(1670.0), Some(450.0)]);
}

#[test]
fn sensor_failing_after_discovery_goes_unavailable_while_others_keep_updating() {
    let fake = machine();
    let mut sensors = collector(&fake);
    let before = sensors.sample();
    let dir = chip_dir(5);
    fake.remove(&format!("{dir}/temp3_input"));
    fake.remove(&format!("{dir}/temp4_input"));
    fake.write(&format!("{dir}/temp4_input/unreadable"), "");
    fake.write(&format!("{dir}/temp1_input"), "60000\n");
    fake.write(&format!("{}/temp1_input", chip_dir(3)), "34850\n");

    let snapshot = sensors.sample();

    assert_eq!(temp(&snapshot, "k10temp/Tccd1").value, None);
    assert_eq!(temp(&snapshot, "k10temp/Tccd2").value, None);
    assert_eq!(temp(&snapshot, "k10temp/Tctl").value, Some(60.0));
    assert_eq!(temp(&snapshot, "nvme/2O412LA45EG7").value, Some(34.85));
    assert_eq!(
        snapshot.temps.len(),
        before.temps.len(),
        "no row disappears with its value"
    );
}

#[test]
fn failed_fan_read_is_unavailable_and_keeps_the_fan_listed() {
    let fake = machine();
    fake.write(&format!("{}/fan4_input", chip_dir(0)), "900\n");
    fake.write(&format!("{}/fan2_input", chip_dir(0)), "not a number\n");
    let mut sensors = collector(&fake);
    fake.write(&format!("{}/fan3_input", chip_dir(0)), "\n");

    let fans = sensors.sample().fans;

    assert_eq!(
        fans,
        vec![
            reading("nct6799/fan2", "CPU FAN", None),
            reading("nct6799/fan3", "CHASSIS FAN", None),
            reading("nct6799/fan4", "NCT6799 FAN4", Some(900.0)),
        ]
    );
}

#[test]
fn override_label_wins() {
    let fake = machine();
    let overrides = BTreeMap::from([
        ("k10temp/Tctl".to_owned(), "PACKAGE".to_owned()),
        ("nct6799/CPUTIN".to_owned(), "SOCKET".to_owned()),
        (format!("nvme/{XPG_SERIAL}"), "BOOT SSD".to_owned()),
        ("spd5118/2".to_owned(), "RAM B".to_owned()),
        ("nct6799/fan4".to_owned(), "PUMP".to_owned()),
    ]);

    let snapshot = SensorCollector::new(&fake.root(), &overrides).sample();

    assert_eq!(temp(&snapshot, "k10temp/Tctl").label, "PACKAGE");
    assert_eq!(temp(&snapshot, "nvme/2O412LA45EG7").label, "BOOT SSD");
    assert_eq!(temp(&snapshot, "spd5118/2").label, "RAM B");
    assert_eq!(temp(&snapshot, "k10temp/Tccd1").label, "CCD0");
    assert_eq!(
        temp(&snapshot, "nct6799/CPUTIN"),
        &reading("nct6799/CPUTIN", "SOCKET", Some(35.0)),
        "an override lists a super-I/O temperature that has no built-in label"
    );
    assert!(
        snapshot
            .temps
            .iter()
            .all(|reading| reading.id != "nct6799/temp2")
    );
    assert_eq!(
        snapshot.fans.last(),
        Some(&reading("nct6799/fan4", "PUMP", Some(0.0))),
        "a user-labelled fan is listed even while idle"
    );
}

#[test]
fn hotplug_on_rescan() {
    let fake = FakeRoot::new();
    write_chip(&fake, 0, "k10temp", &[("temp1_input", "50000")]);
    let mut sensors = collector(&fake);
    write_nvme(
        &fake,
        1,
        0,
        XPG_SERIAL,
        XPG_MODEL,
        &[("Composite", "40000")],
    );

    for _ in 0..RESCAN_EVERY {
        assert_eq!(sensors.sample().temps.len(), 1);
    }
    let snapshot = sensors.sample();

    assert_eq!(snapshot.temps.len(), 2);
    assert_eq!(temp(&snapshot, "nvme/2O412LA45EG7").value, Some(40.0));
}

#[test]
fn chips_are_ordered_by_hwmon_index_not_name() {
    let fake = FakeRoot::new();
    write_chip(&fake, 10, "k10temp", &[("temp1_input", "50000")]);
    write_chip(&fake, 2, "r8169_0_e00:00", &[("temp1_input", "40000")]);

    let snapshot = collector(&fake).sample();

    let ids: Vec<&str> = snapshot.temps.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, ["r8169_0_e00:00/temp1", "k10temp/temp1"]);
}

#[test]
fn dimms_are_numbered_by_device_path_not_hwmon_index() {
    let fake = FakeRoot::new();
    write_dimm(&fake, 7, "2-0053", "30000");
    write_dimm(&fake, 8, "2-0051", "32000");

    let snapshot = collector(&fake).sample();

    assert_eq!(
        snapshot.temps,
        vec![
            reading("spd5118/2", "DIMM 2", Some(30.0)),
            reading("spd5118/1", "DIMM 1", Some(32.0)),
        ]
    );
}

#[test]
fn listed_fan_stays_listed_and_new_spinning_fan_joins_on_rescan() {
    let fake = FakeRoot::new();
    write_chip(
        &fake,
        0,
        "nct6799",
        &[("fan1_input", "0"), ("fan4_input", "1200")],
    );
    let mut sensors = collector(&fake);
    assert_eq!(
        sensors.sample().fans,
        vec![reading("nct6799/fan4", "NCT6799 FAN4", Some(1200.0))]
    );
    fake.write(&format!("{}/fan1_input", chip_dir(0)), "800\n");
    fake.write(&format!("{}/fan4_input", chip_dir(0)), "0\n");

    let fans = sample_through_rescan(&mut sensors).fans;

    assert_eq!(
        fans,
        vec![
            reading("nct6799/fan1", "NCT6799 FAN1", Some(800.0)),
            reading("nct6799/fan4", "NCT6799 FAN4", Some(0.0)),
        ]
    );
}

#[test]
fn super_io_chips_are_read_every_fifth_sample() {
    let fake = FakeRoot::new();
    write_chip(
        &fake,
        0,
        "nct6799",
        &[
            ("temp1_label", "SYSTIN"),
            ("temp1_input", "36000"),
            ("fan2_input", "1670"),
        ],
    );
    write_chip(
        &fake,
        1,
        "k10temp",
        &[("temp1_label", "Tctl"), ("temp1_input", "50000")],
    );
    let mut sensors = collector(&fake);
    let first = sensors.sample();
    assert_eq!(temp(&first, "nct6799/SYSTIN").value, Some(36.0));
    assert_eq!(fan_values(&first), [Some(1670.0)]);
    fake.write(&format!("{}/temp1_input", chip_dir(0)), "40000\n");
    fake.write(&format!("{}/fan2_input", chip_dir(0)), "1700\n");
    fake.write(&format!("{}/temp1_input", chip_dir(1)), "51000\n");

    for _ in 1..SUPERIO_READ_EVERY {
        let snapshot = sensors.sample();
        assert_eq!(temp(&snapshot, "nct6799/SYSTIN").value, Some(36.0));
        assert_eq!(fan_values(&snapshot), [Some(1670.0)]);
        assert_eq!(temp(&snapshot, "k10temp/Tctl").value, Some(51.0));
    }
    let snapshot = sensors.sample();

    assert_eq!(temp(&snapshot, "nct6799/SYSTIN").value, Some(40.0));
    assert_eq!(fan_values(&snapshot), [Some(1700.0)]);
}

#[test]
fn failed_super_io_read_stays_unavailable_until_the_next_read() {
    let fake = FakeRoot::new();
    write_chip(
        &fake,
        0,
        "nct6799",
        &[("temp1_label", "SYSTIN"), ("temp1_input", "36000")],
    );
    let mut sensors = collector(&fake);
    let systin = |sensors: &mut SensorCollector| temp(&sensors.sample(), "nct6799/SYSTIN").value;
    assert_eq!(systin(&mut sensors), Some(36.0));
    let input = format!("{}/temp1_input", chip_dir(0));
    fake.remove(&input);
    for _ in 1..SUPERIO_READ_EVERY {
        assert_eq!(systin(&mut sensors), Some(36.0));
    }

    assert_eq!(systin(&mut sensors), None);
    fake.write(&input, "37000\n");
    for _ in 1..SUPERIO_READ_EVERY {
        assert_eq!(systin(&mut sensors), None);
    }
    assert_eq!(systin(&mut sensors), Some(37.0));
}

#[test]
fn repeated_chip_names_get_an_ordinal_by_device_path() {
    let fake = FakeRoot::new();
    for (hwmon_index, pci_function, tctl, other) in
        [(3, "19.3", "60000", "45000"), (4, "18.3", "50000", "44000")]
    {
        write_chip(
            &fake,
            hwmon_index,
            "k10temp",
            &[
                ("temp1_label", "Tctl"),
                ("temp1_input", tctl),
                ("temp2_input", other),
            ],
        );
        fake.link_dir(
            &format!("{}/device", chip_dir(hwmon_index)),
            &format!("sys/devices/pci0000:00/0000:00:{pci_function}"),
        );
    }

    let snapshot = collector(&fake).sample();

    assert_eq!(
        snapshot.temps,
        vec![
            reading("k10temp#2/Tctl", "CPU TCTL", Some(60.0)),
            reading("k10temp#2/temp2", "K10TEMP#2 TEMP2", Some(45.0)),
            reading("k10temp/Tctl", "CPU TCTL", Some(50.0)),
            reading("k10temp/temp2", "K10TEMP TEMP2", Some(44.0)),
        ]
    );
}

#[test]
fn nvme_model_is_uppercased_and_serial_kept_verbatim() {
    let fake = FakeRoot::new();
    write_nvme(
        &fake,
        0,
        0,
        "abc123Xy",
        "samsung ssd 980 pro",
        &[("Composite", "40000")],
    );

    let snapshot = collector(&fake).sample();

    assert_eq!(
        snapshot.temps,
        vec![reading("nvme/abc123Xy", "SAMSUNG SSD 980 PRO", Some(40.0))]
    );
}

#[test]
fn rescan_that_finds_no_chips_keeps_the_previous_sensors() {
    let fake = FakeRoot::new();
    let chip = [("temp1_label", "Tctl"), ("temp1_input", "50000")];
    write_chip(&fake, 0, "k10temp", &chip);
    let mut sensors = collector(&fake);
    fake.remove(HWMON_DIR);

    let snapshot = sample_through_rescan(&mut sensors);

    assert_eq!(
        snapshot.temps,
        vec![reading("k10temp/Tctl", "CPU TCTL", None)]
    );
    write_chip(&fake, 0, "k10temp", &chip);
    assert_eq!(sensors.sample().temps[0].value, Some(50.0));
}

#[test]
fn every_it8xxx_chip_is_a_super_io_chip() {
    let fake = FakeRoot::new();
    for (index, name, rpm) in [(0, "it8728", "1200"), (1, "it8628", "1500")] {
        write_chip(
            &fake,
            index,
            name,
            &[("temp1_input", "40000"), ("fan1_input", rpm)],
        );
    }
    let mut sensors = collector(&fake);
    let first = sensors.sample();
    fake.write(&format!("{}/fan1_input", chip_dir(0)), "1300\n");
    fake.write(&format!("{}/fan1_input", chip_dir(1)), "1600\n");

    let second = sensors.sample();

    assert!(first.temps.is_empty(), "unnamed super-I/O temps are hidden");
    assert_eq!(
        first.fans,
        vec![
            reading("it8728/fan1", "IT8728 FAN1", Some(1200.0)),
            reading("it8628/fan1", "IT8628 FAN1", Some(1500.0)),
        ]
    );
    assert_eq!(
        fan_values(&second),
        [Some(1200.0), Some(1500.0)],
        "read on a slow cadence"
    );
}
