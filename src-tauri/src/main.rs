#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod commands;
mod db;
mod error;
mod process;
mod scanner;
mod state;
mod types;

use commands::*;
use rusqlite::Connection;
use std::path::PathBuf;
use tauri::Manager;

fn main() {
    // 初始化数据库
    let app_dir = tauri::api::path::home_dir()
        .map(|h| h.join(".mem_snapshot_tool"))
        .unwrap_or_else(|| PathBuf::from("."));

    std::fs::create_dir_all(&app_dir).ok();
    let db_path = app_dir.join("snapshots.db");
    let conn = Connection::open(&db_path).expect("无法打开数据库");
    db::init_db(&conn).expect("数据库初始化失败");

    let app_state = state::AppState::new(conn);

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            list_all_processes,
            list_processes_by_category,
            get_process_regions,
            read_process_memory,
            scan_memory,
            create_memory_snapshot,
            auto_snapshot_at_address,
            list_snapshots,
            get_snapshot,
            delete_snapshot,
            update_snapshot_note,
            count_snapshots,
            inject_snapshot_to_process,
            set_active_pid,
            get_active_pid,
            set_monitored_pids,
            get_monitored_pids,
            toggle_floating_panel,
            show_floating_panel,
            hide_floating_panel,
        ])
        .setup(|app| {
            // 设置全局快捷键
            setup_global_shortcut(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
