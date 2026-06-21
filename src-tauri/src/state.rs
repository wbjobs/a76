use parking_lot::RwLock;
use rusqlite::Connection;
use std::sync::Arc;

pub struct AppState {
    pub db: Arc<RwLock<Connection>>,
    pub active_pid: RwLock<Option<u32>>,
    pub monitored_pids: RwLock<Vec<u32>>,
}

impl AppState {
    pub fn new(db: Connection) -> Self {
        Self {
            db: Arc::new(RwLock::new(db)),
            active_pid: RwLock::new(None),
            monitored_pids: RwLock::new(Vec::new()),
        }
    }
}
