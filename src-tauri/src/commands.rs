use crate::db;
use crate::error::{AppError, AppResult};
use crate::process::{list_processes, memory_regions, read_memory, write_memory};
use crate::scanner::{scan_process, snapshot_region};
use crate::state::AppState;
use crate::types::{
    DataBlock, InjectionResult, MemoryRegion, ProcessInfo, ScanConfig, Snapshot,
    SnapshotCreateParams,
};
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

#[derive(Serialize, Deserialize)]
pub struct ListSnapshotQuery {
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[tauri::command]
pub async fn list_all_processes() -> AppResult<Vec<ProcessInfo>> {
    let mut list = list_processes()?;
    list.sort_by(|a, b| {
        b.memory_mb
            .partial_cmp(&a.memory_mb)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(list)
}

#[tauri::command]
pub async fn list_processes_by_category(
    categories: Vec<String>,
) -> AppResult<Vec<ProcessInfo>> {
    use crate::types::ProcessCategory;
    let all = list_processes()?;
    let set: std::collections::HashSet<String> = categories.into_iter().collect();
    Ok(all
        .into_iter()
        .filter(|p| {
            let label = p.category.label();
            set.contains("*")
                || set.contains(label)
                || set.contains(&match p.category {
                    ProcessCategory::Browser => "browser".into(),
                    ProcessCategory::IDE => "ide".into(),
                    ProcessCategory::Design => "design".into(),
                    ProcessCategory::Other => "other".into(),
                })
        })
        .collect())
}

#[tauri::command]
pub async fn get_process_regions(pid: u32) -> AppResult<Vec<MemoryRegion>> {
    memory_regions(pid)
}

#[tauri::command]
pub async fn read_process_memory(pid: u32, address: u64, size: u32) -> AppResult<Vec<u8>> {
    read_memory(pid, address, size as usize)
}

#[tauri::command]
pub async fn scan_memory(config: ScanConfig) -> AppResult<Vec<DataBlock>> {
    scan_process(&config)
}

#[tauri::command]
pub async fn create_memory_snapshot(
    state: State<'_, AppState>,
    params: SnapshotCreateParams,
) -> AppResult<i64> {
    let db = state.db.read();
    db::create_snapshot(&db, &params)
}

#[tauri::command]
pub async fn auto_snapshot_at_address(
    state: State<'_, AppState>,
    pid: u32,
    process_name: String,
    address: u64,
    size: u32,
    data_type: crate::types::DataType,
    note: Option<String>,
) -> AppResult<i64> {
    let (raw, hex) = snapshot_region(pid, address, size as usize)?;
    let content = match std::str::from_utf8(&raw) {
        Ok(s) => {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
                serde_json::to_string_pretty(&v).unwrap_or_else(|_| s.to_string())
            } else {
                s.to_string()
            }
        }
        Err(_) => format!("Binary data ({} bytes)", raw.len()),
    };

    let params = SnapshotCreateParams {
        process_name,
        pid,
        address,
        size: size as usize,
        data_type,
        content,
        raw_data: hex,
        note,
    };
    let db = state.db.read();
    db::create_snapshot(&db, &params)
}

#[tauri::command]
pub async fn list_snapshots(
    state: State<'_, AppState>,
    query: Option<ListSnapshotQuery>,
) -> AppResult<Vec<Snapshot>> {
    let q = query.unwrap_or(ListSnapshotQuery {
        pid: None,
        process_name: None,
        limit: Some(100),
        offset: Some(0),
    });
    let db = state.db.read();
    db::list_snapshots(
        &db,
        q.pid,
        q.process_name.as_deref(),
        q.limit.unwrap_or(100),
        q.offset.unwrap_or(0),
    )
}

#[tauri::command]
pub async fn get_snapshot(state: State<'_, AppState>, id: i64) -> AppResult<Option<Snapshot>> {
    let db = state.db.read();
    db::get_snapshot(&db, id)
}

#[tauri::command]
pub async fn delete_snapshot(state: State<'_, AppState>, id: i64) -> AppResult<bool> {
    let db = state.db.write();
    db::delete_snapshot(&db, id)
}

#[tauri::command]
pub async fn update_snapshot_note(
    state: State<'_, AppState>,
    id: i64,
    note: String,
) -> AppResult<bool> {
    let db = state.db.write();
    db::update_snapshot_note(&db, id, &note)
}

#[tauri::command]
pub async fn count_snapshots(
    state: State<'_, AppState>,
    query: Option<ListSnapshotQuery>,
) -> AppResult<i64> {
    let q = query.unwrap_or(ListSnapshotQuery {
        pid: None,
        process_name: None,
        limit: None,
        offset: None,
    });
    let db = state.db.read();
    db::count_snapshots(&db, q.pid, q.process_name.as_deref())
}

#[tauri::command]
pub async fn inject_snapshot_to_process(
    snapshot_id: i64,
    target_pid: u32,
    target_address: Option<u64>,
    state: State<'_, AppState>,
) -> AppResult<InjectionResult> {
    // 1. 获取快照
    let db = state.db.read();
    let snap = db::get_snapshot(&db, snapshot_id)?
        .ok_or(AppError::InvalidSnapshotId(snapshot_id))?;
    drop(db);

    // 2. 解码 raw_data (hex -> bytes)
    let raw_bytes = hex_to_bytes(&snap.raw_data)
        .ok_or_else(|| AppError::Other("无法解析快照原始数据".into()))?;

    // 3. 确定写入地址
    let write_addr = target_address.unwrap_or(snap.address);

    // 4. 校验目标地址是否可写（可选）
    match memory_regions(target_pid) {
        Ok(regions) => {
            let contains = regions.iter().any(|r| {
                write_addr >= r.base_address
                    && write_addr < r.base_address + r.size
                    && r.is_writable
            });
            if !contains {
                return Ok(InjectionResult {
                    success: false,
                    address: write_addr,
                    bytes_written: 0,
                    message: format!("地址 0x{:X} 不在可写区域内，已中止注入以避免崩溃", write_addr),
                });
            }
        }
        Err(e) => {
            return Ok(InjectionResult {
                success: false,
                address: write_addr,
                bytes_written: 0,
                message: format!("无法访问目标进程内存区域: {}", e),
            });
        }
    }

    // 5. 执行写入
    match write_memory(target_pid, write_addr, &raw_bytes) {
        Ok(bytes_written) => Ok(InjectionResult {
            success: true,
            address: write_addr,
            bytes_written,
            message: format!(
                "成功将快照 #{} ({} bytes) 注入到 PID={} 的 0x{:X}",
                snapshot_id, bytes_written, target_pid, write_addr
            ),
        }),
        Err(e) => Ok(InjectionResult {
            success: false,
            address: write_addr,
            bytes_written: 0,
            message: format!("注入失败: {}", e),
        }),
    }
}

fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    let clean: String = hex
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .collect();
    if clean.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(clean.len() / 2);
    let bytes = clean.as_bytes();
    for i in (0..bytes.len()).step_by(2) {
        let hi = (bytes[i] as char).to_digit(16)?;
        let lo = (bytes[i + 1] as char).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
    }
    Some(out)
}

#[tauri::command]
pub async fn set_active_pid(state: State<'_, AppState>, pid: Option<u32>) -> AppResult<()> {
    let mut lock = state.active_pid.write();
    *lock = pid;
    Ok(())
}

#[tauri::command]
pub async fn get_active_pid(state: State<'_, AppState>) -> AppResult<Option<u32>> {
    Ok(state.active_pid.read().clone())
}

#[tauri::command]
pub async fn set_monitored_pids(
    state: State<'_, AppState>,
    pids: Vec<u32>,
) -> AppResult<()> {
    let mut lock = state.monitored_pids.write();
    *lock = pids;
    Ok(())
}

#[tauri::command]
pub async fn get_monitored_pids(state: State<'_, AppState>) -> AppResult<Vec<u32>> {
    Ok(state.monitored_pids.read().clone())
}

#[tauri::command]
pub fn toggle_floating_panel(app: tauri::AppHandle) -> AppResult<()> {
    if let Some(win) = app.get_window("floating-panel") {
        match win.is_visible() {
            Ok(true) => {
                let _ = win.hide();
            }
            _ => {
                let _ = win.show();
                let _ = win.set_focus();
                let _ = win.center();
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn show_floating_panel(app: tauri::AppHandle) -> AppResult<()> {
    if let Some(win) = app.get_window("floating-panel") {
        let _ = win.show();
        let _ = win.set_focus();
        let _ = win.center();
    }
    Ok(())
}

#[tauri::command]
pub async fn hide_floating_panel(app: tauri::AppHandle) -> AppResult<()> {
    if let Some(win) = app.get_window("floating-panel") {
        let _ = win.hide();
    }
    Ok(())
}

pub fn setup_global_shortcut(app: &mut tauri::App) -> AppResult<()> {
    let app_handle = app.handle();
    let shortcut = app
        .global_shortcut_manager()
        .register("Ctrl+Shift+Alt+M", move || {
            // 呼出浮动面板
            let ah = app_handle.clone();
            std::thread::spawn(move || {
                if let Some(win) = ah.get_window("floating-panel") {
                    match win.is_visible() {
                        Ok(true) => {
                            let _ = win.hide();
                        }
                        _ => {
                            let _ = win.show();
                            let _ = win.set_focus();
                            let _ = win.center();
                        }
                    }
                }
            });
        });
    if let Err(e) = shortcut {
        return Err(AppError::HotkeyError(format!(
            "注册全局快捷键 Ctrl+Shift+Alt+M 失败: {}",
            e
        )));
    }

    // 第二个快捷键：快速扫描当前激活进程
    let app_handle2 = app.handle();
    let _ = app
        .global_shortcut_manager()
        .register("Ctrl+Shift+Alt+S", move || {
            let ah = app_handle2.clone();
            std::thread::spawn(move || {
                if let Some(win) = ah.get_window("main") {
                    let _ = win.emit("shortcut-quick-scan", ());
                }
                if let Some(win) = ah.get_window("floating-panel") {
                    let _ = win.emit("shortcut-quick-scan", ());
                }
            });
        });

    Ok(())
}
