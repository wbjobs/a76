# MemSnapshot 私有云同步服务器

基于 WebSocket + MongoDB 的端到端加密同步服务。

## 架构

```
┌─────────────┐   WebSocket (加密传输)   ┌──────────────┐
│ 办公室电脑  │ ───────────────────────> │  Node.js 服务 │
│ (Tauri App) │                          │   + MongoDB  │
└─────────────┘ <─────────────────────── └──────────────┘
       ^                                         ^
       │                                         │
       └─────────────────────────────────────────┘
                        │
┌─────────────┐                          
│  家里电脑   │
│ (Tauri App) │
└─────────────┘
```

## 安全特性

- **端到端 AES-256-GCM 加密**：服务器存储的是加密后的快照数据，无法解密读取
- **密钥派生**：用户输入的密码通过 PBKDF2 (10万次迭代) 派生为 256 位密钥
- **完整性校验**：GCM 认证标签保证数据未被篡改
- **设备认证**：每个设备有唯一 ID，服务端记录设备登录历史

## 启动

```bash
cd sync-server
cp .env.example .env
# 编辑 .env，配置 MongoDB 地址

npm install
npm start
```

## 协议

### 认证
```json
{ "type": "auth", "device_id": "xxx", "user_key": "64位hex密钥", "device_name": "办公室电脑" }
```

### 推送快照
```json
{ "type": "push_snapshots", "snapshots": [{ "snapshot_id": "...", "encrypted_data": "...", "data_hash": "...", "data_size": 1234 }] }
```

### 拉取快照
```json
{ "type": "pull_snapshots", "after": "2024-01-01T00:00:00Z", "limit": 100 }
```

### 获取快照ID列表
```json
{ "type": "list_snapshot_ids" }
```

### 删除快照
```json
{ "type": "delete_snapshots", "snapshot_ids": ["id1", "id2"] }
```
