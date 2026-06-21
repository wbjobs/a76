use crate::error::{AppError, AppResult};
use crate::types::{InjectionLog, InjectionStep, Snapshot, SnapshotCreateParams};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

pub fn init_db(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS snapshots (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            process_name TEXT NOT NULL,
            pid INTEGER NOT NULL,
            address INTEGER NOT NULL,
            size INTEGER NOT NULL,
            data_type TEXT NOT NULL,
            content TEXT,
            raw_data TEXT NOT NULL,
            note TEXT,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_snapshots_pid ON snapshots(pid);
        CREATE INDEX IF NOT EXISTS idx_snapshots_process ON snapshots(process_name);
        CREATE INDEX IF NOT EXISTS idx_snapshots_created ON snapshots(created_at DESC);

        CREATE TABLE IF NOT EXISTS injection_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            snapshot_id INTEGER NOT NULL,
            target_pid INTEGER NOT NULL,
            target_address INTEGER NOT NULL,
            data_size INTEGER NOT NULL,
            temp_alloc_address INTEGER,
            temp_alloc_size INTEGER,
            memcpy_address INTEGER,
            thread_exit_code INTEGER,
            success INTEGER NOT NULL DEFAULT 0,
            rolled_back INTEGER NOT NULL DEFAULT 0,
            steps_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (snapshot_id) REFERENCES snapshots(id)
        );

        CREATE INDEX IF NOT EXISTS idx_injection_logs_pid ON injection_logs(target_pid);
        CREATE INDEX IF NOT EXISTS idx_injection_logs_snapshot ON injection_logs(snapshot_id);
        CREATE INDEX IF NOT EXISTS idx_injection_logs_created ON injection_logs(created_at DESC);
        "#,
    )?;
    Ok(())
}

pub fn create_snapshot(conn: &Connection, params: &SnapshotCreateParams) -> AppResult<i64> {
    let now = Utc::now().to_rfc3339();
    let dtype = serde_json::to_string(&params.data_type)?;
    conn.execute(
        r#"INSERT INTO snapshots (process_name, pid, address, size, data_type, content, raw_data, note, created_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"#,
        params![
            params.process_name,
            params.pid as i64,
            params.address as i64,
            params.size as i64,
            dtype,
            params.content,
            params.raw_data,
            params.note,
            now,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn row_to_snapshot(row: &rusqlite::Row) -> rusqlite::Result<Snapshot> {
    let dtype_str: String = row.get(5)?;
    let data_type = serde_json::from_str(&dtype_str).unwrap_or(crate::types::DataType::Json);
    Ok(Snapshot {
        id: row.get(0)?,
        process_name: row.get(1)?,
        pid: row.get::<_, i64>(2)? as u32,
        address: row.get::<_, i64>(3)? as u64,
        size: row.get::<_, i64>(4)? as usize,
        data_type,
        content: row.get(6)?,
        raw_data: row.get(7)?,
        note: row.get(8)?,
        created_at: row.get(9)?,
    })
}

pub fn list_snapshots(
    conn: &Connection,
    pid: Option<u32>,
    process_name: Option<&str>,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<Snapshot>> {
    let mut sql =
        "SELECT id, process_name, pid, address, size, data_type, content, raw_data, note, created_at FROM snapshots WHERE 1=1"
            .to_string();
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(p) = pid {
        sql.push_str(" AND pid = ?");
        params_vec.push(Box::new(p as i64));
    }
    if let Some(pn) = process_name {
        sql.push_str(" AND process_name LIKE ?");
        params_vec.push(Box::new(format!("%{}%", pn)));
    }
    sql.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");
    params_vec.push(Box::new(limit));
    params_vec.push(Box::new(offset));

    let refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| &**b).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(refs.as_slice(), row_to_snapshot)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get_snapshot(conn: &Connection, id: i64) -> AppResult<Option<Snapshot>> {
    let mut stmt = conn.prepare(
        "SELECT id, process_name, pid, address, size, data_type, content, raw_data, note, created_at FROM snapshots WHERE id = ?",
    )?;
    let res = stmt
        .query_row(params![id], row_to_snapshot)
        .optional()?;
    Ok(res)
}

pub fn delete_snapshot(conn: &Connection, id: i64) -> AppResult<bool> {
    let count = conn.execute("DELETE FROM snapshots WHERE id = ?", params![id])?;
    Ok(count > 0)
}

pub fn update_snapshot_note(conn: &Connection, id: i64, note: &str) -> AppResult<bool> {
    let count = conn.execute("UPDATE snapshots SET note = ? WHERE id = ?", params![note, id])?;
    if count == 0 {
        return Err(AppError::InvalidSnapshotId(id));
    }
    Ok(true)
}

pub fn count_snapshots(
    conn: &Connection,
    pid: Option<u32>,
    process_name: Option<&str>,
) -> AppResult<i64> {
    let mut sql = "SELECT COUNT(*) FROM snapshots WHERE 1=1".to_string();
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(p) = pid {
        sql.push_str(" AND pid = ?");
        params_vec.push(Box::new(p as i64));
    }
    if let Some(pn) = process_name {
        sql.push_str(" AND process_name LIKE ?");
        params_vec.push(Box::new(format!("%{}%", pn)));
    }

    let refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| &**b).collect();
    let count: i64 = conn.query_row(&sql, refs.as_slice(), |r| r.get(0))?;
    Ok(count)
}

// ===== 注入日志 CRUD =====

pub struct CreateInjectionLogParams {
    pub snapshot_id: i64,
    pub target_pid: u32,
    pub target_address: u64,
    pub data_size: usize,
    pub temp_alloc_address: Option<u64>,
    pub temp_alloc_size: Option<usize>,
    pub memcpy_address: Option<u64>,
    pub thread_exit_code: Option<u32>,
    pub success: bool,
    pub rolled_back: bool,
    pub steps: Vec<InjectionStep>,
}

pub fn create_injection_log(conn: &Connection, p: &CreateInjectionLogParams) -> AppResult<i64> {
    let now = Utc::now().to_rfc3339();
    let steps_json = serde_json::to_string(&p.steps)?;
    conn.execute(
        r#"INSERT INTO injection_logs
           (snapshot_id, target_pid, target_address, data_size, temp_alloc_address,
            temp_alloc_size, memcpy_address, thread_exit_code, success, rolled_back, steps_json, created_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"#,
        params![
            p.snapshot_id as i64,
            p.target_pid as i64,
            p.target_address as i64,
            p.data_size as i64,
            p.temp_alloc_address.map(|v| v as i64),
            p.temp_alloc_size.map(|v| v as i64),
            p.memcpy_address.map(|v| v as i64),
            p.thread_exit_code.map(|v| v as i64),
            p.success as i32,
            p.rolled_back as i32,
            steps_json,
            now,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn row_to_injection_log(row: &rusqlite::Row) -> rusqlite::Result<InjectionLog> {
    Ok(InjectionLog {
        id: row.get(0)?,
        snapshot_id: row.get(1)?,
        target_pid: row.get::<_, i64>(2)? as u32,
        target_address: row.get::<_, i64>(3)? as u64,
        data_size: row.get::<_, i64>(4)? as usize,
        temp_alloc_address: row.get::<_, Option<i64>>(5)?.map(|v| v as u64),
        temp_alloc_size: row.get::<_, Option<i64>>(6)?.map(|v| v as usize),
        memcpy_address: row.get::<_, Option<i64>>(7)?.map(|v| v as u64),
        thread_exit_code: row.get::<_, Option<i64>>(8)?,
        success: row.get::<_, i32>(9)? != 0,
        rolled_back: row.get::<_, i32>(10)? != 0,
        steps_json: row.get(11)?,
        created_at: row.get(12)?,
    })
}

pub fn list_injection_logs(
    conn: &Connection,
    target_pid: Option<u32>,
    snapshot_id: Option<i64>,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<InjectionLog>> {
    let mut sql = "SELECT id, snapshot_id, target_pid, target_address, data_size, temp_alloc_address, temp_alloc_size, memcpy_address, thread_exit_code, success, rolled_back, steps_json, created_at FROM injection_logs WHERE 1=1".to_string();
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(p) = target_pid {
        sql.push_str(" AND target_pid = ?");
        params_vec.push(Box::new(p as i64));
    }
    if let Some(s) = snapshot_id {
        sql.push_str(" AND snapshot_id = ?");
        params_vec.push(Box::new(s));
    }
    sql.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");
    params_vec.push(Box::new(limit));
    params_vec.push(Box::new(offset));

    let refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| &**b).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(refs.as_slice(), row_to_injection_log)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get_injection_log(conn: &Connection, id: i64) -> AppResult<Option<InjectionLog>> {
    let mut stmt = conn.prepare(
        "SELECT id, snapshot_id, target_pid, target_address, data_size, temp_alloc_address, temp_alloc_size, memcpy_address, thread_exit_code, success, rolled_back, steps_json, created_at FROM injection_logs WHERE id = ?",
    )?;
    let res = stmt
        .query_row(params![id], row_to_injection_log)
        .optional()?;
    Ok(res)
}
