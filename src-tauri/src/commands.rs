use crate::crypto::{derive_key_from_password, generate_device_id, generate_hex_key, sha256_hex};
use crate::db;
use crate::db::CreateInjectionLogParams;
use crate::error::{AppError, AppResult};
use crate::process::{
    create_remote_thread_and_wait, get_module_proc_address, list_processes, memory_regions,
    read_memory, virtual_alloc_ex, virtual_free_ex, write_memory,
};
use crate::scanner::{scan_process, snapshot_region};
use crate::state::AppState;
use crate::sync::SyncClient;
use crate::types::{
    DataBlock, DiffChunk, DiffResult, InjectionLog, InjectionResult, InjectionStep, MemoryRegion,
    ProcessInfo, ScanConfig, Snapshot, SnapshotCreateParams, SyncConfigInfo, SyncConfigParams,
    SyncResult, SyncStatus,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

fn now_ts() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn ok_step(name: &str, addr: Option<u64>, size: Option<usize>, rv: Option<String>) -> InjectionStep {
    InjectionStep {
        step: name.to_string(),
        success: true,
        address: addr,
        size,
        return_value: rv,
        error: None,
        timestamp: now_ts(),
    }
}

fn err_step(name: &str, addr: Option<u64>, size: Option<usize>, err: String) -> InjectionStep {
    InjectionStep {
        step: name.to_string(),
        success: false,
        address: addr,
        size,
        return_value: None,
        error: Some(err),
        timestamp: now_ts(),
    }
}

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

// ===== 安全注入核心 =====

#[tauri::command]
pub async fn inject_snapshot_to_process(
    snapshot_id: i64,
    target_pid: u32,
    target_address: Option<u64>,
    state: State<'_, AppState>,
) -> AppResult<InjectionResult> {
    let mut steps: Vec<InjectionStep> = Vec::new();
    let mut temp_alloc_addr: Option<u64> = None;
    let mut memcpy_addr_val: Option<u64> = None;
    let mut thread_exit: Option<u32> = None;
    let mut rolled_back = false;

    // ===== Step 0: 获取快照数据 =====
    let db_conn = state.db.read();
    let snap = db::get_snapshot(&db_conn, snapshot_id)?;
    drop(db_conn);

    let snap = match snap {
        Some(s) => s,
        None => {
            steps.push(err_step("获取快照", None, None, format!("快照 #{} 不存在", snapshot_id)));
            return Ok(InjectionResult {
                success: false,
                address: 0,
                bytes_written: 0,
                message: format!("快照 #{} 不存在", snapshot_id),
                log_id: None,
                steps,
                rolled_back: false,
                temp_alloc_address: None,
                memcpy_result: None,
            });
        }
    };

    let raw_bytes = match hex_to_bytes(&snap.raw_data) {
        Some(b) => b,
        None => {
            steps.push(err_step("解码快照数据", None, None, "无法解析HEX原始数据".into()));
            return Ok(InjectionResult {
                success: false,
                address: 0,
                bytes_written: 0,
                message: "无法解析快照原始数据".into(),
                log_id: None,
                steps,
                rolled_back: false,
                temp_alloc_address: None,
                memcpy_result: None,
            });
        }
    };

    let data_size = raw_bytes.len();
    let write_addr = target_address.unwrap_or(snap.address);

    steps.push(ok_step(
        "获取快照",
        Some(snap.address),
        Some(data_size),
        Some(format!("snapshot_id={}, data_size={}", snapshot_id, data_size)),
    ));

    // ===== Step 1: 校验目标地址是否可写 =====
    match memory_regions(target_pid) {
        Ok(regions) => {
            let contains = regions.iter().any(|r| {
                write_addr >= r.base_address
                    && write_addr < r.base_address + r.size
                    && r.is_writable
            });
            if !contains {
                steps.push(err_step(
                    "校验目标地址",
                    Some(write_addr),
                    Some(data_size),
                    format!("0x{:X} 不在可写内存区域，中止注入", write_addr),
                ));
                let log_id = save_injection_log(
                    &state, snapshot_id, target_pid, write_addr, data_size,
                    None, None, None, None, false, false, &steps,
                )?;
                return Ok(InjectionResult {
                    success: false,
                    address: write_addr,
                    bytes_written: 0,
                    message: format!("地址 0x{:X} 不在可写区域内，已中止", write_addr),
                    log_id: Some(log_id),
                    steps,
                    rolled_back: false,
                    temp_alloc_address: None,
                    memcpy_result: None,
                });
            }
            steps.push(ok_step(
                "校验目标地址",
                Some(write_addr),
                Some(data_size),
                Some(format!("0x{:X} 在可写区域内，保护=RW", write_addr)),
            ));
        }
        Err(e) => {
            steps.push(err_step("校验目标地址", Some(write_addr), None, format!("{}", e)));
            let log_id = save_injection_log(
                &state, snapshot_id, target_pid, write_addr, data_size,
                None, None, None, None, false, false, &steps,
            )?;
            return Ok(InjectionResult {
                success: false,
                address: write_addr,
                bytes_written: 0,
                message: format!("无法访问目标进程内存区域: {}", e),
                log_id: Some(log_id),
                steps,
                rolled_back: false,
                temp_alloc_address: None,
                memcpy_result: None,
            });
        }
    }

    // ===== Step 2: 备份目标地址原始数据（用于回滚） =====
    let backup_data = match read_memory(target_pid, write_addr, data_size) {
        Ok(data) => {
            steps.push(ok_step(
                "备份目标原始数据",
                Some(write_addr),
                Some(data.len()),
                Some(format!("已读取 {} 字节", data.len())),
            ));
            Some(data)
        }
        Err(e) => {
            steps.push(err_step("备份目标原始数据", Some(write_addr), Some(data_size), format!("{}", e)));
            None
        }
    };

    // ===== Step 3: VirtualAllocEx 在目标进程分配临时内存 =====
    let alloc_size = data_size + 64; // 额外空间给 memcpy 参数结构
    match virtual_alloc_ex(target_pid, alloc_size) {
        Ok(addr) => {
            temp_alloc_addr = Some(addr);
            steps.push(ok_step(
                "VirtualAllocEx 分配临时区",
                Some(addr),
                Some(alloc_size),
                Some(format!("临时区=0x{:X}, size={}", addr, alloc_size)),
            ));
        }
        Err(e) => {
            steps.push(err_step("VirtualAllocEx 分配临时区", None, Some(alloc_size), format!("{}", e)));
            let log_id = save_injection_log(
                &state, snapshot_id, target_pid, write_addr, data_size,
                None, None, None, None, false, false, &steps,
            )?;
            return Ok(InjectionResult {
                success: false,
                address: write_addr,
                bytes_written: 0,
                message: format!("VirtualAllocEx 分配临时区失败: {}", e),
                log_id: Some(log_id),
                steps,
                rolled_back: false,
                temp_alloc_address: None,
                memcpy_result: None,
            });
        }
    }

    let temp_addr = temp_alloc_addr.unwrap();

    // ===== Step 4: 写入快照数据到临时区 =====
    match write_memory(target_pid, temp_addr, &raw_bytes) {
        Ok(written) => {
            steps.push(ok_step(
                "WriteProcessMemory 到临时区",
                Some(temp_addr),
                Some(written),
                Some(format!("已写入 {} 字节到 0x{:X}", written, temp_addr)),
            ));
        }
        Err(e) => {
            steps.push(err_step("WriteProcessMemory 到临时区", Some(temp_addr), Some(data_size), format!("{}", e)));
            // 回滚：释放临时区
            let _ = virtual_free_ex(target_pid, temp_addr, 0);
            rolled_back = true;
            steps.push(ok_step("回滚-释放临时区", Some(temp_addr), None, Some("VirtualFreeEx 已调用".into())));
            let log_id = save_injection_log(
                &state, snapshot_id, target_pid, write_addr, data_size,
                temp_alloc_addr, Some(alloc_size), None, None, false, rolled_back, &steps,
            )?;
            return Ok(InjectionResult {
                success: false,
                address: write_addr,
                bytes_written: 0,
                message: format!("写入临时区失败，已自动回滚并释放临时内存: {}", e),
                log_id: Some(log_id),
                steps,
                rolled_back,
                temp_alloc_address: temp_alloc_addr,
                memcpy_result: None,
            });
        }
    }

    // ===== Step 5: 获取 kernel32!memcpy 地址 =====
    match get_module_proc_address("kernel32.dll", "CopyMemory") {
        Ok(addr) => {
            memcpy_addr_val = Some(addr);
            steps.push(ok_step(
                "获取 kernel32!CopyMemory 地址",
                Some(addr),
                None,
                Some(format!("CopyMemory=0x{:X}", addr)),
            ));
        }
        Err(e) => {
            steps.push(err_step("获取 memcpy 地址", None, None, format!("{}", e)));
            // 回退方案：直接 WriteProcessMemory 到目标地址
            steps.push(ok_step("回退方案", None, None, Some("远程线程不可用，使用直接 WriteProcessMemory".into())));

            match write_memory(target_pid, write_addr, &raw_bytes) {
                Ok(written) => {
                    steps.push(ok_step(
                        "直接 WriteProcessMemory",
                        Some(write_addr),
                        Some(written),
                        Some(format!("已直接写入 {} 字节", written)),
                    ));
                    let _ = virtual_free_ex(target_pid, temp_addr, 0);
                    steps.push(ok_step("释放临时区", Some(temp_addr), None, Some("已释放".into())));

                    let log_id = save_injection_log(
                        &state, snapshot_id, target_pid, write_addr, data_size,
                        temp_alloc_addr, Some(alloc_size), None, None, true, false, &steps,
                    )?;
                    return Ok(InjectionResult {
                        success: true,
                        address: write_addr,
                        bytes_written: written,
                        message: format!(
                            "通过直接写入模式成功注入 {} 字节到 PID={} 的 0x{:X}",
                            written, target_pid, write_addr
                        ),
                        log_id: Some(log_id),
                        steps,
                        rolled_back: false,
                        temp_alloc_address: temp_alloc_addr,
                        memcpy_result: None,
                    });
                }
                Err(e2) => {
                    steps.push(err_step("直接 WriteProcessMemory", Some(write_addr), Some(data_size), format!("{}", e2)));
                    let _ = virtual_free_ex(target_pid, temp_addr, 0);
                    rolled_back = true;
                    steps.push(ok_step("回滚-释放临时区", Some(temp_addr), None, Some("已释放".into())));

                    // 尝试恢复备份
                    if let Some(ref backup) = backup_data {
                        match write_memory(target_pid, write_addr, backup) {
                            Ok(_) => steps.push(ok_step("回滚-恢复原始数据", Some(write_addr), Some(backup.len()), Some("已恢复".into()))),
                            Err(re) => steps.push(err_step("回滚-恢复原始数据", Some(write_addr), None, format!("{}", re))),
                        }
                    }

                    let log_id = save_injection_log(
                        &state, snapshot_id, target_pid, write_addr, data_size,
                        temp_alloc_addr, Some(alloc_size), None, None, false, rolled_back, &steps,
                    )?;
                    return Ok(InjectionResult {
                        success: false,
                        address: write_addr,
                        bytes_written: 0,
                        message: format!("注入失败，已自动回滚: {}", e2),
                        log_id: Some(log_id),
                        steps,
                        rolled_back,
                        temp_alloc_address: temp_alloc_addr,
                        memcpy_result: None,
                    });
                }
            }
        }
    }

    // ===== Step 6: 构造 memcpy 参数并写入目标进程 =====
    // memcpy(dest, src, size) - 我们需要构造一个参数块
    // 使用 RemoteThread 参数传递技巧：
    //   由于 CreateRemoteThread 只能传1个参数，我们用 temp 区末尾放参数结构
    //   实际上对于 CopyMemory(RtlMoveMemory)，它是一个 macro 展开为 __movsb
    //   更安全的做法：把 memcpy 参数块写到临时区，然后用 NtCreateThreadEx 或 shellcode
    //   简化方案：直接用 RtlMoveMemory 函数地址创建远程线程
    //
    //   但 RtlMoveMemory(dst, src, len) 有3个参数，不能直接通过 CreateRemoteThread 传。
    //   解决方案：写一小段 shellcode 到临时区，调用 RtlMoveMemory 后返回。
    //
    //   x64 shellcode:
    //     mov rcx, <dest>      ; 48 B9 <8 bytes>
    //     mov rdx, <src>       ; 48 BA <8 bytes>
    //     mov r8, <size>       ; 49 B8 <8 bytes>
    //     sub rsp, 0x28        ; 48 83 EC 28
    //     call <RtlMoveMemory> ; 48 FF 15 02 00 00 00
    //     add rsp, 0x28        ; 48 83 C4 28
    //     ret                  ; C3
    //     <8 byte absolute address of RtlMoveMemory>

    let memcpy_fn = memcpy_addr_val.unwrap();

    // 获取 ntdll!RtlMoveMemory (实际实现)
    let rtl_addr = match get_module_proc_address("ntdll.dll", "RtlMoveMemory") {
        Ok(a) => {
            steps.push(ok_step(
                "获取 ntdll!RtlMoveMemory",
                Some(a),
                None,
                Some(format!("RtlMoveMemory=0x{:X}", a)),
            ));
            a
        }
        Err(_) => {
            steps.push(ok_step(
                "获取 ntdll!RtlMoveMemory",
                Some(memcpy_fn),
                None,
                Some("回退到 kernel32!CopyMemory".into()),
            ));
            memcpy_fn
        }
    };

    // 构建x64 shellcode
    let mut shellcode: Vec<u8> = Vec::with_capacity(64);

    // mov rcx, <write_addr>
    shellcode.extend_from_slice(&[0x48, 0xB9]);
    shellcode.extend_from_slice(&write_addr.to_le_bytes());

    // mov rdx, <temp_addr>
    shellcode.extend_from_slice(&[0x48, 0xBA]);
    shellcode.extend_from_slice(&temp_addr.to_le_bytes());

    // mov r8, <data_size>
    shellcode.extend_from_slice(&[0x49, 0xB8]);
    shellcode.extend_from_slice(&(data_size as u64).to_le_bytes());

    // sub rsp, 0x28 (shadow space + alignment)
    shellcode.extend_from_slice(&[0x48, 0x83, 0xEC, 0x28]);

    // call [rip+2] -> 读取后面8字节绝对地址
    shellcode.extend_from_slice(&[0xFF, 0x15, 0x02, 0x00, 0x00, 0x00]);

    // add rsp, 0x28
    shellcode.extend_from_slice(&[0x48, 0x83, 0xC4, 0x28]);

    // ret
    shellcode.push(0xC3);

    // 8 bytes: absolute address of RtlMoveMemory
    shellcode.extend_from_slice(&rtl_addr.to_le_bytes());

    // 写 shellcode 到临时区末尾
    let shellcode_offset = data_size; // 放在数据之后
    let shellcode_addr = temp_addr + shellcode_offset as u64;

    match write_memory(target_pid, shellcode_addr, &shellcode) {
        Ok(written) => {
            steps.push(ok_step(
                "写入 shellcode",
                Some(shellcode_addr),
                Some(written),
                Some(format!("shellcode {} bytes at 0x{:X}", written, shellcode_addr)),
            ));
        }
        Err(e) => {
            steps.push(err_step("写入 shellcode", Some(shellcode_addr), Some(shellcode.len()), format!("{}", e)));
            let _ = virtual_free_ex(target_pid, temp_addr, 0);
            rolled_back = true;
            steps.push(ok_step("回滚-释放临时区", Some(temp_addr), None, Some("已释放".into())));
            if let Some(ref backup) = backup_data {
                let _ = write_memory(target_pid, write_addr, backup);
                steps.push(ok_step("回滚-恢复原始数据", Some(write_addr), None, Some("已恢复".into())));
            }
            let log_id = save_injection_log(
                &state, snapshot_id, target_pid, write_addr, data_size,
                temp_alloc_addr, Some(alloc_size), memcpy_addr_val, None, false, rolled_back, &steps,
            )?;
            return Ok(InjectionResult {
                success: false,
                address: write_addr,
                bytes_written: 0,
                message: format!("写入 shellcode 失败，已自动回滚: {}", e),
                log_id: Some(log_id),
                steps,
                rolled_back,
                temp_alloc_address: temp_alloc_addr,
                memcpy_result: None,
            });
        }
    }

    // ===== Step 7: CreateRemoteThread 执行 shellcode =====
    match create_remote_thread_and_wait(target_pid, shellcode_addr, 0) {
        Ok(exit_code) => {
            thread_exit = Some(exit_code);
            steps.push(ok_step(
                "CreateRemoteThread",
                Some(shellcode_addr),
                None,
                Some(format!("远程线程退出码={}", exit_code)),
            ));
        }
        Err(e) => {
            steps.push(err_step("CreateRemoteThread", Some(shellcode_addr), None, format!("{}", e)));
            // 回滚
            let _ = virtual_free_ex(target_pid, temp_addr, 0);
            rolled_back = true;
            steps.push(ok_step("回滚-释放临时区", Some(temp_addr), None, Some("已释放".into())));
            if let Some(ref backup) = backup_data {
                let _ = write_memory(target_pid, write_addr, backup);
                steps.push(ok_step("回滚-恢复原始数据", Some(write_addr), None, Some("已恢复".into())));
            }
            let log_id = save_injection_log(
                &state, snapshot_id, target_pid, write_addr, data_size,
                temp_alloc_addr, Some(alloc_size), memcpy_addr_val, thread_exit, false, rolled_back, &steps,
            )?;
            return Ok(InjectionResult {
                success: false,
                address: write_addr,
                bytes_written: 0,
                message: format!("远程线程执行失败，已自动回滚: {}", e),
                log_id: Some(log_id),
                steps,
                rolled_back,
                temp_alloc_address: temp_alloc_addr,
                memcpy_result: thread_exit,
            });
        }
    }

    // ===== Step 8: 验证写入 - 读取目标地址并比对 =====
    match read_memory(target_pid, write_addr, data_size) {
        Ok(verify_data) => {
            let match_count = verify_data
                .iter()
                .zip(raw_bytes.iter())
                .filter(|(a, b)| a == b)
                .count();
            let verify_pct = (match_count as f64 / data_size as f64) * 100.0;
            steps.push(ok_step(
                "验证写入",
                Some(write_addr),
                Some(data_size),
                Some(format!("匹配率={:.1}% ({}/{})", verify_pct, match_count, data_size)),
            ));
        }
        Err(e) => {
            steps.push(err_step("验证写入", Some(write_addr), Some(data_size), format!("{}", e)));
        }
    }

    // ===== Step 9: 清理 - 释放临时区 =====
    match virtual_free_ex(target_pid, temp_addr, 0) {
        Ok(()) => {
            steps.push(ok_step("释放临时区", Some(temp_addr), None, Some("VirtualFreeEx 成功".into())));
        }
        Err(e) => {
            steps.push(err_step("释放临时区", Some(temp_addr), None, format!("释放失败: {}", e)));
        }
    }

    let log_id = save_injection_log(
        &state, snapshot_id, target_pid, write_addr, data_size,
        temp_alloc_addr, Some(alloc_size), memcpy_addr_val, thread_exit, true, false, &steps,
    )?;

    Ok(InjectionResult {
        success: true,
        address: write_addr,
        bytes_written: data_size,
        message: format!(
            "安全注入成功：通过 VirtualAllocEx + shellcode(RtlMoveMemory) 注入 {} 字节到 PID={} 的 0x{:X}",
            data_size, target_pid, write_addr
        ),
        log_id: Some(log_id),
        steps,
        rolled_back: false,
        temp_alloc_address: temp_alloc_addr,
        memcpy_result: thread_exit,
    })
}

fn save_injection_log(
    state: &State<'_, AppState>,
    snapshot_id: i64,
    target_pid: u32,
    target_address: u64,
    data_size: usize,
    temp_alloc_address: Option<u64>,
    temp_alloc_size: Option<usize>,
    memcpy_address: Option<u64>,
    thread_exit_code: Option<u32>,
    success: bool,
    rolled_back: bool,
    steps: &[InjectionStep],
) -> AppResult<i64> {
    let db_conn = state.db.read();
    let log_params = CreateInjectionLogParams {
        snapshot_id,
        target_pid,
        target_address,
        data_size,
        temp_alloc_address,
        temp_alloc_size,
        memcpy_address,
        thread_exit_code,
        success,
        rolled_back,
        steps: steps.to_vec(),
    };
    db::create_injection_log(&db_conn, &log_params)
}

// ===== 注入日志查询命令 =====

#[tauri::command]
pub async fn list_injection_logs(
    state: State<'_, AppState>,
    target_pid: Option<u32>,
    snapshot_id: Option<i64>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> AppResult<Vec<InjectionLog>> {
    let db = state.db.read();
    db::list_injection_logs(&db, target_pid, snapshot_id, limit.unwrap_or(100), offset.unwrap_or(0))
}

#[tauri::command]
pub async fn get_injection_log(state: State<'_, AppState>, id: i64) -> AppResult<Option<InjectionLog>> {
    let db = state.db.read();
    db::get_injection_log(&db, id)
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

// ===== 同步配置命令 =====

#[tauri::command]
pub async fn get_sync_config(state: State<'_, AppState>) -> AppResult<Option<SyncConfigInfo>> {
    let db = state.db.read();
    let config = db::get_sync_config(&db)?;
    Ok(config.map(|c| SyncConfigInfo {
        server_address: c.server_address,
        device_id: c.device_id,
        device_name: c.device_name,
        auto_sync: c.auto_sync,
        last_sync: c.last_sync,
        updated_at: c.updated_at,
        has_key: !c.encryption_key.is_empty(),
    }))
}

#[tauri::command]
pub async fn set_sync_config(state: State<'_, AppState>, params: SyncConfigParams) -> AppResult<SyncConfigInfo> {
    let derived_key = derive_key_from_password(&params.encryption_password);
    let encryption_key = hex::encode(derived_key);
    
    let device_id = {
        let db = state.db.read();
        db::get_sync_config(&db)?
            .map(|c| c.device_id)
            .unwrap_or_else(generate_device_id)
    };

    let config = db::SyncConfig {
        server_address: params.server_address,
        encryption_key,
        device_id: device_id.clone(),
        device_name: params.device_name,
        auto_sync: params.auto_sync,
        last_sync: None,
        updated_at: Utc::now().to_rfc3339(),
    };

    {
        let db = state.db.write();
        db::set_sync_config(&db, &config)?;
    }

    Ok(SyncConfigInfo {
        server_address: config.server_address,
        device_id: config.device_id,
        device_name: config.device_name,
        auto_sync: config.auto_sync,
        last_sync: config.last_sync,
        updated_at: config.updated_at,
        has_key: true,
    })
}

#[tauri::command]
pub async fn clear_sync_config(state: State<'_, AppState>) -> AppResult<()> {
    let mut sync_client = state.sync_client.lock().await;
    if let Some(mut client) = sync_client.take() {
        let _ = client.disconnect().await;
    }
    
    let db = state.db.write();
    db.execute("DELETE FROM sync_config WHERE id = 1", [])?;
    Ok(())
}

// ===== 同步连接命令 =====

#[tauri::command]
pub async fn connect_sync(state: State<'_, AppState>) -> AppResult<SyncStatus> {
    let config = {
        let db = state.db.read();
        db::get_sync_config(&db)?
            .ok_or_else(|| AppError::Other("请先配置同步服务器".into()))?
    };

    let mut sync_client = SyncClient::new(config);
    sync_client.connect().await?;
    
    let status = sync_client.get_status();
    *state.sync_client.lock().await = Some(sync_client);
    
    Ok(status)
}

#[tauri::command]
pub async fn disconnect_sync(state: State<'_, AppState>) -> AppResult<SyncStatus> {
    let mut sync_client_opt = state.sync_client.lock().await;
    if let Some(mut client) = sync_client_opt.take() {
        let _ = client.disconnect().await;
        return Ok(SyncStatus {
            connected: false,
            server_address: client.get_status().server_address,
            last_sync: None,
            cloud_snapshot_count: 0,
            error: None,
        });
    }
    
    Ok(SyncStatus {
        connected: false,
        server_address: String::new(),
        last_sync: None,
        cloud_snapshot_count: 0,
        error: None,
    })
}

#[tauri::command]
pub async fn get_sync_status(state: State<'_, AppState>) -> AppResult<SyncStatus> {
    let sync_client_opt = state.sync_client.lock().await;
    if let Some(client) = sync_client_opt.as_ref() {
        return Ok(client.get_status());
    }
    
    let db = state.db.read();
    let config = db::get_sync_config(&db)?;
    Ok(SyncStatus {
        connected: false,
        server_address: config.map(|c| c.server_address).unwrap_or_default(),
        last_sync: config.and_then(|c| c.last_sync),
        cloud_snapshot_count: 0,
        error: None,
    })
}

// ===== 同步操作命令 =====

#[tauri::command]
pub async fn sync_push_all(state: State<'_, AppState>) -> AppResult<SyncResult> {
    let sync_client = state.sync_client.lock().await;
    let client = sync_client.as_ref()
        .ok_or_else(|| AppError::Other("未连接同步服务器".into()))?;

    let snapshots = {
        let db = state.db.read();
        db::list_snapshots(&db, None, None, 1000, 0)?
    };

    if snapshots.is_empty() {
        return Ok(SyncResult {
            success: true,
            pushed: 0,
            pulled: 0,
            error: None,
        });
    }

    let result = client.push_snapshots(&snapshots).await?;
    let pushed_count = result["results"].as_array()
        .map(|arr| arr.iter().filter(|r| r["error"].is_null()).count() as i64)
        .unwrap_or(0);

    {
        let db = state.db.write();
        db::update_sync_last_sync(&db, &Utc::now().to_rfc3339())?;
    }

    Ok(SyncResult {
        success: true,
        pushed: pushed_count,
        pulled: 0,
        error: None,
    })
}

#[tauri::command]
pub async fn sync_pull_all(state: State<'_, AppState>) -> AppResult<SyncResult> {
    let sync_client = state.sync_client.lock().await;
    let client = sync_client.as_ref()
        .ok_or_else(|| AppError::Other("未连接同步服务器".into()))?;

    let config = {
        let db = state.db.read();
        db::get_sync_config(&db)?
    };

    let after = config.as_ref().and_then(|c| c.last_sync.clone());
    let cloud_snapshots = client.pull_snapshots(after, 500).await?;

    let mut inserted = 0i64;
    {
        let db = state.db.write();
        for snap in &cloud_snapshots {
            let exists = db.query_row(
                "SELECT 1 FROM snapshots WHERE id = ?",
                [snap.id],
                |_| Ok(true),
            ).optional()?.unwrap_or(false);
            
            if !exists {
                let params = SnapshotCreateParams {
                    process_name: snap.process_name.clone(),
                    pid: snap.pid,
                    address: snap.address,
                    size: snap.size,
                    data_type: snap.data_type,
                    content: snap.content.clone(),
                    raw_data: snap.raw_data.clone(),
                    note: snap.note.clone(),
                };
                db::create_snapshot(&db, &params)?;
                inserted += 1;
            }
        }
        
        db::update_sync_last_sync(&db, &Utc::now().to_rfc3339())?;
    }

    Ok(SyncResult {
        success: true,
        pushed: 0,
        pulled: inserted,
        error: None,
    })
}

#[tauri::command]
pub async fn sync_push_ids(state: State<'_, AppState>, snapshot_ids: Vec<i64>) -> AppResult<SyncResult> {
    let sync_client = state.sync_client.lock().await;
    let client = sync_client.as_ref()
        .ok_or_else(|| AppError::Other("未连接同步服务器".into()))?;

    let mut snapshots = Vec::new();
    {
        let db = state.db.read();
        for id in snapshot_ids {
            if let Some(snap) = db::get_snapshot(&db, id)? {
                snapshots.push(snap);
            }
        }
    }

    if snapshots.is_empty() {
        return Ok(SyncResult {
            success: true,
            pushed: 0,
            pulled: 0,
            error: None,
        });
    }

    let result = client.push_snapshots(&snapshots).await?;
    let pushed_count = result["results"].as_array()
        .map(|arr| arr.iter().filter(|r| r["error"].is_null()).count() as i64)
        .unwrap_or(0);

    {
        let db = state.db.write();
        db::update_sync_last_sync(&db, &Utc::now().to_rfc3339())?;
    }

    Ok(SyncResult {
        success: true,
        pushed: pushed_count,
        pulled: 0,
        error: None,
    })
}

#[tauri::command]
pub async fn generate_encryption_key() -> AppResult<String> {
    Ok(generate_hex_key())
}

// ===== 快照差异对比 =====

fn compute_diff(old_bytes: &[u8], new_bytes: &[u8]) -> Vec<DiffChunk> {
    let mut chunks = Vec::new();
    let min_len = old_bytes.len().min(new_bytes.len());
    
    let mut i = 0;
    while i < min_len {
        if old_bytes[i] != new_bytes[i] {
            let start = i;
            while i < min_len && old_bytes[i] != new_bytes[i] {
                i += 1;
            }
            let end = i;
            
            let old_hex = hex::encode(&old_bytes[start..end.min(old_bytes.len())]);
            let new_hex = hex::encode(&new_bytes[start..end.min(new_bytes.len())]);
            
            chunks.push(DiffChunk {
                kind: "modify".into(),
                old_start: start,
                old_end: end.min(old_bytes.len()),
                new_start: start,
                new_end: end.min(new_bytes.len()),
                old_content: old_hex,
                new_content: new_hex,
            });
        } else {
            i += 1;
        }
    }

    if old_bytes.len() > new_bytes.len() {
        chunks.push(DiffChunk {
            kind: "delete".into(),
            old_start: new_bytes.len(),
            old_end: old_bytes.len(),
            new_start: new_bytes.len(),
            new_end: new_bytes.len(),
            old_content: hex::encode(&old_bytes[new_bytes.len()..]),
            new_content: String::new(),
        });
    } else if new_bytes.len() > old_bytes.len() {
        chunks.push(DiffChunk {
            kind: "insert".into(),
            old_start: old_bytes.len(),
            old_end: old_bytes.len(),
            new_start: old_bytes.len(),
            new_end: new_bytes.len(),
            old_content: String::new(),
            new_content: hex::encode(&new_bytes[old_bytes.len()..]),
        });
    }

    chunks
}

fn bytes_to_preview(bytes: &[u8], max_len: usize) -> String {
    let take = bytes.len().min(max_len);
    let hex = hex::encode(&bytes[..take]);
    if bytes.len() > max_len {
        format!("{}... ({} bytes total)", hex, bytes.len())
    } else {
        hex
    }
}

fn calculate_similarity(old: &[u8], new: &[u8]) -> f64 {
    if old.is_empty() && new.is_empty() {
        return 1.0;
    }
    if old.is_empty() || new.is_empty() {
        return 0.0;
    }
    
    let min_len = old.len().min(new.len());
    let mut matches = 0;
    for i in 0..min_len {
        if old[i] == new[i] {
            matches += 1;
        }
    }
    
    let max_len = old.len().max(new.len());
    matches as f64 / max_len as f64
}

#[tauri::command]
pub async fn diff_snapshots(state: State<'_, AppState>, old_id: i64, new_id: i64) -> AppResult<DiffResult> {
    let db = state.db.read();
    
    let old_snap = db::get_snapshot(&db, old_id)?
        .ok_or_else(|| AppError::InvalidSnapshotId(old_id))?;
    let new_snap = db::get_snapshot(&db, new_id)?
        .ok_or_else(|| AppError::InvalidSnapshotId(new_id))?;
    
    let old_bytes = hex::decode(&old_snap.raw_data)
        .map_err(|e| AppError::Other(format!("旧快照数据解码失败: {}", e)))?;
    let new_bytes = hex::decode(&new_snap.raw_data)
        .map_err(|e| AppError::Other(format!("新快照数据解码失败: {}", e)))?;
    
    let chunks = compute_diff(&old_bytes, &new_bytes);
    let changed_bytes: usize = chunks.iter()
        .map(|c| (c.old_end - c.old_start).max(c.new_end - c.new_start))
        .sum();
    let similarity = calculate_similarity(&old_bytes, &new_bytes);
    
    Ok(DiffResult {
        old_id,
        new_id,
        old_size: old_bytes.len(),
        new_size: new_bytes.len(),
        changed_bytes,
        similarity,
        chunks,
        old_preview: bytes_to_preview(&old_bytes, 256),
        new_preview: bytes_to_preview(&new_bytes, 256),
    })
}
