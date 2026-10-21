use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CpuSnapshot {
    pub model: String,
    pub threads: Vec<f32>,
    pub ccds: Vec<CcdLoad>,
    pub load: f32,
    pub freq_mhz: u32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CcdLoad {
    pub id: u8,
    pub cpus: Vec<u16>,
    pub l3_kib: u32,
    pub vcache: bool,
    pub load: f32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MemSnapshot {
    pub total_kib: u64,
    pub used_kib: u64,
    pub swap_total_kib: u64,
    pub swap_used_kib: u64,
    pub top: Vec<ProcUsage>,
}

/// `cpu` and `mem` are both percentages of the whole machine.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProcUsage {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,
    pub mem: f32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SensorSnapshot {
    pub temps: Vec<Reading>,
    pub fans: Vec<Reading>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Reading {
    pub id: String,
    pub label: String,
    pub value: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GpuSnapshot {
    pub present: bool,
    pub name: String,
    pub load: f32,
    pub vram_used_mib: u32,
    pub vram_total_mib: u32,
    pub temp_c: f32,
    pub power_w: f32,
    pub power_limit_w: f32,
    pub fan_pct: f32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DiskSnapshot {
    pub drives: Vec<Drive>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Drive {
    pub id: String,
    pub label: String,
    pub model: String,
    pub kernel_name: String,
    pub state: DriveState,
    pub mounts: Vec<String>,
    pub used_pct: Option<f32>,
    pub temp_c: Option<f32>,
    pub read_bps: u64,
    pub write_bps: u64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriveState {
    #[default]
    Live,
    Dead,
    Unmounted,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PerfSnapshot {
    pub daemon_cpu: f32,
    pub daemon_rss_kib: u64,
    pub shell_cpu: Option<f32>,
    pub shell_rss_kib: Option<u64>,
}
