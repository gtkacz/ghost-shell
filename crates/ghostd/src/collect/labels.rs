const K10TEMP_CHIP: &str = "k10temp";
const NUVOTON_CHIP_PREFIX: &str = "nct";
const ITE_CHIP_PREFIX: &str = "it8";
const REALTEK_NIC_CHIP_PREFIX: &str = "r8169";

struct LabelRule {
    chip_prefix: &'static str,
    sensor: Option<&'static str>,
    label: &'static str,
}

const LABEL_RULES: &[LabelRule] = &[
    LabelRule {
        chip_prefix: K10TEMP_CHIP,
        sensor: Some("Tctl"),
        label: "CPU TCTL",
    },
    LabelRule {
        chip_prefix: K10TEMP_CHIP,
        sensor: Some("Tccd1"),
        label: "CCD0",
    },
    LabelRule {
        chip_prefix: K10TEMP_CHIP,
        sensor: Some("Tccd2"),
        label: "CCD1",
    },
    LabelRule {
        chip_prefix: NUVOTON_CHIP_PREFIX,
        sensor: Some("SYSTIN"),
        label: "MOBO",
    },
    LabelRule {
        chip_prefix: NUVOTON_CHIP_PREFIX,
        sensor: Some("fan2"),
        label: "CPU FAN",
    },
    LabelRule {
        chip_prefix: NUVOTON_CHIP_PREFIX,
        sensor: Some("fan3"),
        label: "CHASSIS FAN",
    },
    LabelRule {
        chip_prefix: REALTEK_NIC_CHIP_PREFIX,
        sensor: None,
        label: "NIC",
    },
];

pub(super) fn is_super_io(chip: &str) -> bool {
    [NUVOTON_CHIP_PREFIX, ITE_CHIP_PREFIX]
        .iter()
        .any(|prefix| chip.starts_with(prefix))
}

/// The built-in label for a chip's sensor, where `sensor` is the raw `*_label` text or, without
/// one, the input's `tempN`/`fanN` name.
pub(super) fn known_label(chip: &str, sensor: &str) -> Option<&'static str> {
    LABEL_RULES
        .iter()
        .find(|rule| {
            chip.starts_with(rule.chip_prefix)
                && rule.sensor.is_none_or(|expected| expected == sensor)
        })
        .map(|rule| rule.label)
}

pub(super) fn fallback_label(chip: &str, sensor: &str) -> String {
    format!("{chip} {sensor}").to_uppercase()
}

pub(super) fn drive_label(model: &str) -> String {
    model.to_uppercase()
}

pub(super) fn dimm_label(position: usize) -> String {
    format!("DIMM {position}")
}
