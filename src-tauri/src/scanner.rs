use crate::error::{AppError, AppResult};
use crate::process::{memory_regions, read_memory};
use crate::types::{DataBlock, DataType, ScanConfig, ScanPattern, PatternKind};
use base64::Engine;
use regex::bytes::RegexSet;
use serde_json::Value as JsonValue;

pub fn scan_process(config: &ScanConfig) -> AppResult<Vec<DataBlock>> {
    let regions = memory_regions(config.pid).map_err(|e| {
        AppError::MemoryScanFailed(format!("获取内存区域失败: {}", e))
    })?;

    let mut results: Vec<DataBlock> = Vec::new();

    // 编译正则模式
    let regex_patterns: Vec<&ScanPattern> = config
        .patterns
        .iter()
        .filter(|p| p.kind == PatternKind::Regex)
        .collect();
    let regex_set = if !regex_patterns.is_empty() {
        let exprs: Vec<&str> = regex_patterns.iter().map(|p| p.value.as_str()).collect();
        Some(RegexSet::new(exprs)?)
    } else {
        None
    };

    let detect_json = config.patterns.iter().any(|p| p.kind == PatternKind::Json);
    let detect_pickle = config.patterns.iter().any(|p| p.kind == PatternKind::Pickle);
    let detect_base64 = config.patterns.iter().any(|p| p.kind == PatternKind::Base64);

    let max_size = config.max_region_size_mb.unwrap_or(4) * 1024 * 1024;

    for region in regions {
        // 应用范围过滤
        if let Some(start) = config.region_start {
            if region.base_address < start {
                continue;
            }
        }
        if let Some(end) = config.region_end {
            if region.base_address > end {
                break;
            }
        }
        if region.size > max_size {
            continue;
        }

        let data = match read_memory(config.pid, region.base_address, region.size as usize) {
            Ok(d) => d,
            Err(_) => continue,
        };

        // JSON 检测
        if detect_json {
            detect_json_blocks(&data, region.base_address, &mut results);
        }

        // Pickle 检测 (Python序列化)
        if detect_pickle {
            detect_pickle_blocks(&data, region.base_address, &mut results);
        }

        // Base64 检测
        if detect_base64 {
            detect_base64_blocks(&data, region.base_address, &mut results);
        }

        // 正则匹配
        if let Some(set) = &regex_set {
            detect_regex_blocks(&data, region.base_address, set, &regex_patterns, &mut results);
        }
    }

    // 按大小降序排序，优先显示大块
    results.sort_by(|a, b| b.size.cmp(&a.size));
    // 去重（同一地址同一类型）
    results.dedup_by(|a, b| a.address == b.address && a.data_type == b.data_type);

    Ok(results)
}

fn detect_json_blocks(data: &[u8], base_addr: u64, results: &mut Vec<DataBlock>) {
    // 简单策略：在文本段中寻找合法的 JSON 对象/数组
    // 1. 找到 "{" 或 "["，然后尝试解析
    let min_size = 10;

    let mut i = 0;
    while i < data.len() {
        if data[i] == b'{' || data[i] == b'[' {
            // 向前扫描找到匹配的闭合括号，最多扫描 256KB
            let max_end = (i + 256 * 1024).min(data.len());
            let mut depth: i32 = 1;
            let mut in_string = false;
            let mut escape = false;
            let mut end = i + 1;

            while end < max_end && depth > 0 {
                let c = data[end];
                if escape {
                    escape = false;
                } else if c == b'\\' && in_string {
                    escape = true;
                } else if c == b'"' {
                    in_string = !in_string;
                } else if !in_string {
                    match c {
                        b'{' | b'[' => depth += 1,
                        b'}' | b']' => depth -= 1,
                        _ => {}
                    }
                }
                if depth == 0 {
                    break;
                }
                end += 1;
            }

            if depth == 0 && end - i + 1 >= min_size {
                let slice = &data[i..=end];
                if let Ok(s) = std::str::from_utf8(slice) {
                    if let Ok(val) = serde_json::from_str::<JsonValue>(s) {
                        let pretty = serde_json::to_string_pretty(&val)
                            .unwrap_or_else(|_| s.to_string());
                        results.push(DataBlock {
                            address: base_addr + i as u64,
                            size: slice.len(),
                            data_type: DataType::Json,
                            content: pretty,
                            raw_hex: bytes_to_hex(slice),
                        });
                        // 跳到该块末尾
                        i = end + 1;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
}

fn detect_pickle_blocks(data: &[u8], base_addr: u64, results: &mut Vec<DataBlock>) {
    // Python pickle 以特定操作码序列开头
    // Pickle 协议签名: 0x80 (PROTO) 后跟协议号 (1-5)，或 0x85 (FRAME)
    let mut i = 0;
    while i + 4 < data.len() {
        let mut detected = false;
        // 检测 PROTO 标记
        if data[i] == 0x80 && data[i + 1] >= 1 && data[i + 1] <= 5 {
            // 向后寻找 STOP (0x2E) 操作码，最多 512KB
            let max_end = (i + 512 * 1024).min(data.len());
            let mut end = i + 2;
            let mut found_stop = false;
            while end < max_end {
                if data[end] == 0x2E {
                    found_stop = true;
                    break;
                }
                end += 1;
            }
            if found_stop && end - i > 10 {
                let slice = &data[i..=end];
                results.push(DataBlock {
                    address: base_addr + i as u64,
                    size: slice.len(),
                    data_type: DataType::Pickle,
                    content: format!("Pickle protocol={}, len={} bytes", data[i + 1], slice.len()),
                    raw_hex: bytes_to_hex(slice),
                });
                i = end + 1;
                detected = true;
            }
        }
        if !detected {
            i += 1;
        }
    }
}

fn detect_base64_blocks(data: &[u8], base_addr: u64, results: &mut Vec<DataBlock>) {
    // Base64 字符集：A-Za-z0-9+/=，至少 16 个字符
    let b64_set: std::collections::HashSet<u8> =
        (b'A'..=b'Z').chain(b'a'..=b'z').chain(b'0'..=b'9').chain(b"+/=".iter().copied()).collect();

    let min_len = 16;
    let mut i = 0;
    while i < data.len() {
        if b64_set.contains(&data[i]) {
            let start = i;
            let mut end = i;
            while end < data.len() && b64_set.contains(&data[end]) {
                end += 1;
            }
            let len = end - start;
            // Base64 长度必须是 4 的倍数（最后可以有 padding）
            if len >= min_len && len % 4 == 0 {
                let slice = &data[start..end];
                if let Ok(s) = std::str::from_utf8(slice) {
                    let engine = base64::engine::general_purpose::STANDARD;
                    if let Ok(decoded) = engine.decode(slice) {
                        let display = if let Ok(txt) = std::str::from_utf8(&decoded) {
                            format!("{} (decoded text: {}...)", s, txt.chars().take(60).collect::<String>())
                        } else {
                            format!("{} ({} binary bytes)", s, decoded.len())
                        };
                        results.push(DataBlock {
                            address: base_addr + start as u64,
                            size: len,
                            data_type: DataType::Base64,
                            content: display,
                            raw_hex: bytes_to_hex(slice),
                        });
                    }
                }
                i = end;
                continue;
            }
            i = end;
        } else {
            i += 1;
        }
    }
}

fn detect_regex_blocks(
    data: &[u8],
    base_addr: u64,
    set: &RegexSet,
    patterns: &[&ScanPattern],
    results: &mut Vec<DataBlock>,
) {
    // 对整个区域应用正则集
    let matches = set.matches(data);
    for (idx, _matched) in matches.iter().enumerate() {
        if idx >= patterns.len() {
            break;
        }
        let pattern = &patterns[idx];
        // 用单独的 Regex 来获取位置信息
        if let Ok(re) = regex::bytes::Regex::new(&pattern.value) {
            for m in re.find_iter(data).take(50) {
                let start = m.start();
                let end = m.end();
                let slice = &data[start..end];
                let display = match std::str::from_utf8(slice) {
                    Ok(s) => s.to_string(),
                    Err(_) => bytes_to_hex(slice),
                };
                results.push(DataBlock {
                    address: base_addr + start as u64,
                    size: end - start,
                    data_type: DataType::RegexMatch,
                    content: format!("Pattern: {} => {}", pattern.value, display),
                    raw_hex: bytes_to_hex(slice),
                });
            }
        }
    }
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let limit = bytes.len().min(256);
    let mut s = String::with_capacity(limit * 2);
    for b in &bytes[..limit] {
        use std::fmt::Write;
        let _ = write!(s, "{:02X}", b);
    }
    if bytes.len() > limit {
        s.push_str(&format!("...(total {} bytes)", bytes.len()));
    }
    s
}

/// 对特定地址执行一次快照，读取当前数据
pub fn snapshot_region(pid: u32, address: u64, size: usize) -> AppResult<(Vec<u8>, String)> {
    let raw = read_memory(pid, address, size)?;
    let hex = bytes_to_hex(&raw);
    Ok((raw, hex))
}
