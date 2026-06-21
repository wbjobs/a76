use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub path: Option<String>,
    pub memory_mb: f64,
    pub category: ProcessCategory,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ProcessCategory {
    Browser,
    IDE,
    Design,
    Other,
}

impl ProcessCategory {
    pub fn from_name(name: &str) -> Self {
        let lower = name.to_lowercase();
        // Browsers
        if lower.contains("chrome")
            || lower.contains("firefox")
            || lower.contains("edge")
            || lower.contains("safari")
            || lower.contains("opera")
            || lower.contains("brave")
            || lower.contains("msedge")
        {
            return ProcessCategory::Browser;
        }
        // IDEs
        if lower.contains("code")
            || lower.contains("vscode")
            || lower.contains("idea")
            || lower.contains("pycharm")
            || lower.contains("webstorm")
            || lower.contains("eclipse")
            || lower.contains("sublime")
            || lower.contains("atom")
            || lower.contains("rider")
            || lower.contains("visualstudio")
            || lower.contains("devenv")
        {
            return ProcessCategory::IDE;
        }
        // Design software
        if lower.contains("photoshop")
            || lower.contains("illustrator")
            || lower.contains("figma")
            || lower.contains("sketch")
            || lower.contains("xd")
            || lower.contains("blender")
            || lower.contains("gimp")
            || lower.contains("inkscape")
            || lower.contains("corel")
            || lower.contains("afterfx")
            || lower.contains("premiere")
        {
            return ProcessCategory::Design;
        }
        ProcessCategory::Other
    }

    pub fn label(&self) -> &'static str {
        match self {
            ProcessCategory::Browser => "浏览器",
            ProcessCategory::IDE => "IDE",
            ProcessCategory::Design => "设计软件",
            ProcessCategory::Other => "其他",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRegion {
    pub base_address: u64,
    pub size: u64,
    pub protection: String,
    pub is_readable: bool,
    pub is_writable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataBlock {
    pub address: u64,
    pub size: usize,
    pub data_type: DataType,
    pub content: String,
    pub raw_hex: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DataType {
    Json,
    Pickle,
    Base64,
    RegexMatch,
}

impl DataType {
    pub fn label(&self) -> &'static str {
        match self {
            DataType::Json => "JSON",
            DataType::Pickle => "Pickle/Python序列化",
            DataType::Base64 => "Base64编码",
            DataType::RegexMatch => "正则匹配",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: i64,
    pub process_name: String,
    pub pid: u32,
    pub address: u64,
    pub size: usize,
    pub data_type: DataType,
    pub content: String,
    pub raw_data: String,
    pub created_at: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotCreateParams {
    pub process_name: String,
    pub pid: u32,
    pub address: u64,
    pub size: usize,
    pub data_type: DataType,
    pub content: String,
    pub raw_data: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanConfig {
    pub pid: u32,
    pub patterns: Vec<ScanPattern>,
    pub max_region_size_mb: Option<u64>,
    pub region_start: Option<u64>,
    pub region_end: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanPattern {
    pub kind: PatternKind,
    pub value: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PatternKind {
    Json,
    Pickle,
    Base64,
    Regex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectionResult {
    pub success: bool,
    pub address: u64,
    pub bytes_written: usize,
    pub message: String,
    pub log_id: Option<i64>,
    pub steps: Vec<InjectionStep>,
    pub rolled_back: bool,
    pub temp_alloc_address: Option<u64>,
    pub memcpy_result: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectionStep {
    pub step: String,
    pub success: bool,
    pub address: Option<u64>,
    pub size: Option<usize>,
    pub return_value: Option<String>,
    pub error: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectionLog {
    pub id: i64,
    pub snapshot_id: i64,
    pub target_pid: u32,
    pub target_address: u64,
    pub data_size: usize,
    pub temp_alloc_address: Option<u64>,
    pub temp_alloc_size: Option<usize>,
    pub memcpy_address: Option<u64>,
    pub thread_exit_code: Option<i64>,
    pub success: bool,
    pub rolled_back: bool,
    pub steps_json: String,
    pub created_at: String,
}
