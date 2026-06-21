use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use futures::{SinkExt, StreamExt};
use url::Url;

use crate::crypto::{encrypt_data, decrypt_data, sha256_hex};
use crate::db::{self, DbSnapshot, SyncConfig};
use crate::error::AppResult;
use crate::types::Snapshot as ApiSnapshot;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SyncStatus {
    pub connected: bool,
    pub server_address: String,
    pub last_sync: Option<String>,
    pub cloud_snapshot_count: i64,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CloudSnapshot {
    pub snapshot_id: String,
    pub encrypted_data: String,
    pub data_hash: String,
    pub data_size: i64,
    pub updated_at: String,
    pub last_sync_device: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct WsMessage {
    #[serde(rename = "type")]
    message_type: String,
    device_id: String,
    user_key: String,
    #[serde(flatten)]
    extra: serde_json::Value,
}

pub struct SyncClient {
    config: SyncConfig,
    ws_stream: Option<Arc<Mutex<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>>>>,
    status: SyncStatus,
}

impl SyncClient {
    pub fn new(config: SyncConfig) -> Self {
        Self {
            status: SyncStatus {
                connected: false,
                server_address: config.server_address.clone(),
                last_sync: None,
                cloud_snapshot_count: 0,
                error: None,
            },
            config,
            ws_stream: None,
        }
    }

    pub fn get_status(&self) -> SyncStatus {
        self.status.clone()
    }

    pub async fn connect(&mut self) -> AppResult<()> {
        let ws_url = Url::parse(&self.config.server_address)
            .map_err(|e| crate::error::AppError::Other(format!("服务器地址无效: {}", e)))?;

        let (ws_stream, _) = connect_async(ws_url).await
            .map_err(|e| crate::error::AppError::Other(format!("连接服务器失败: {}", e)))?;

        let ws_stream = Arc::new(Mutex::new(ws_stream));
        self.ws_stream = Some(ws_stream.clone());

        let auth_msg = serde_json::json!({
            "type": "auth",
            "device_id": self.config.device_id,
            "user_key": self.config.encryption_key,
            "device_name": self.config.device_name,
        });

        let mut stream = ws_stream.lock().await;
        stream.send(Message::Text(auth_msg.to_string())).await
            .map_err(|e| crate::error::AppError::Other(format!("发送认证失败: {}", e)))?;

        while let Some(msg) = stream.next().await {
            let msg = msg.map_err(|e| crate::error::AppError::Other(format!("接收消息失败: {}", e)))?;
            if let Message::Text(text) = msg {
                let resp: serde_json::Value = serde_json::from_str(&text)?;
                if resp["type"] == "auth_ack" {
                    self.status.connected = true;
                    self.status.cloud_snapshot_count = resp["cloud_snapshot_count"].as_i64().unwrap_or(0);
                    self.status.error = None;
                    return Ok(());
                } else if resp["type"] == "error" {
                    return Err(crate::error::AppError::Other(
                        resp["message"].as_str().unwrap_or("认证失败").to_string()
                    ));
                }
            }
        }

        Err(crate::error::AppError::Other("连接被关闭".into()))
    }

    pub async fn disconnect(&mut self) -> AppResult<()> {
        if let Some(stream) = self.ws_stream.take() {
            let mut stream = stream.lock().await;
            let _ = stream.close(None).await;
        }
        self.status.connected = false;
        Ok(())
    }

    async fn send_request(&self, req: serde_json::Value) -> AppResult<serde_json::Value> {
        let stream = self.ws_stream.as_ref()
            .ok_or_else(|| crate::error::AppError::Other("未连接".into()))?;

        let mut ws = stream.lock().await;
        ws.send(Message::Text(req.to_string())).await
            .map_err(|e| crate::error::AppError::Other(format!("发送失败: {}", e)))?;

        while let Some(msg) = ws.next().await {
            let msg = msg.map_err(|e| crate::error::AppError::Other(format!("接收失败: {}", e)))?;
            if let Message::Text(text) = msg {
                let resp: serde_json::Value = serde_json::from_str(&text)?;
                if resp["type"] == "error" {
                    return Err(crate::error::AppError::Other(
                        resp["message"].as_str().unwrap_or("服务器错误").to_string()
                    ));
                }
                return Ok(resp);
            }
        }

        Err(crate::error::AppError::Other("连接被关闭".into()))
    }

    pub async fn push_snapshots(&self, snapshots: &[ApiSnapshot]) -> AppResult<serde_json::Value> {
        let key = hex::decode(&self.config.encryption_key)
            .map_err(|e| crate::error::AppError::Other(format!("密钥格式错误: {}", e)))?;

        let mut encrypted_list = Vec::new();
        for snap in snapshots {
            let snap_db: DbSnapshot = db::snapshot_to_db(snap);
            let encrypted = encrypt_data(&snap_db, &key)?;
            let data_hash = sha256_hex(&encrypted.as_bytes());
            
            encrypted_list.push(serde_json::json!({
                "snapshot_id": snap.id,
                "encrypted_data": encrypted,
                "data_hash": data_hash,
                "data_size": snap.data.len() as i64 / 2,
            }));
        }

        let req = serde_json::json!({
            "type": "push_snapshots",
            "device_id": self.config.device_id,
            "user_key": self.config.encryption_key,
            "snapshots": encrypted_list,
            "sync_token": chrono::Utc::now().to_rfc3339(),
        });

        self.send_request(req).await
    }

    pub async fn pull_snapshots(&self, after: Option<String>, limit: i64) -> AppResult<Vec<ApiSnapshot>> {
        let req = serde_json::json!({
            "type": "pull_snapshots",
            "device_id": self.config.device_id,
            "user_key": self.config.encryption_key,
            "after": after,
            "limit": limit,
        });

        let resp = self.send_request(req).await?;
        let cloud_snaps: Vec<CloudSnapshot> = serde_json::from_value(resp["snapshots"].clone())?;

        let key = hex::decode(&self.config.encryption_key)
            .map_err(|e| crate::error::AppError::Other(format!("密钥格式错误: {}", e)))?;

        let mut results = Vec::new();
        for cloud_snap in cloud_snaps {
            let db_snap: DbSnapshot = decrypt_data(&cloud_snap.encrypted_data, &key)?;
            let api_snap = db::db_to_snapshot(&db_snap);
            results.push(api_snap);
        }

        Ok(results)
    }

    pub async fn list_cloud_snapshot_ids(&self) -> AppResult<Vec<serde_json::Value>> {
        let req = serde_json::json!({
            "type": "list_snapshot_ids",
            "device_id": self.config.device_id,
            "user_key": self.config.encryption_key,
        });

        let resp = self.send_request(req).await?;
        Ok(serde_json::from_value(resp["ids"].clone())?)
    }

    pub async fn delete_cloud_snapshots(&self, ids: &[String]) -> AppResult<serde_json::Value> {
        let req = serde_json::json!({
            "type": "delete_snapshots",
            "device_id": self.config.device_id,
            "user_key": self.config.encryption_key,
            "snapshot_ids": ids,
        });

        self.send_request(req).await
    }

    pub async fn ping(&self) -> AppResult<bool> {
        let req = serde_json::json!({
            "type": "ping",
            "device_id": self.config.device_id,
            "user_key": self.config.encryption_key,
        });

        let resp = self.send_request(req).await?;
        Ok(resp["timestamp"].is_i64())
    }
}
