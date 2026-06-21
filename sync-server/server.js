import 'dotenv/config';
import { WebSocketServer } from 'ws';
import { MongoClient } from 'mongodb';
import chalk from 'chalk';
import { nanoid } from 'nanoid';
import { decrypt, sha256 } from './crypto.js';

const PORT = process.env.PORT || 8080;
const MONGODB_URI = process.env.MONGODB_URI || 'mongodb://localhost:27017';
const MONGODB_DB = process.env.MONGODB_DB || 'memsnapshot_sync';

let db;
const clients = new Map();

async function initMongo() {
  const client = new MongoClient(MONGODB_URI);
  await client.connect();
  db = client.db(MONGODB_DB);
  
  await db.collection('snapshots').createIndex({ user_key: 1, snapshot_id: 1 }, { unique: true });
  await db.collection('snapshots').createIndex({ user_key: 1, updated_at: -1 });
  await db.collection('devices').createIndex({ user_key: 1, device_id: 1 }, { unique: true });
  
  console.log(chalk.green('✓ MongoDB 连接成功'));
}

function send(ws, type, payload = {}) {
  ws.send(JSON.stringify({ type, ...payload }));
}

function error(ws, message) {
  send(ws, 'error', { message });
}

async function handleMessage(ws, msg) {
  const { type, device_id, user_key, ...rest } = JSON.parse(msg);
  
  if (!type || !device_id || !user_key) {
    return error(ws, '缺少必填字段: type, device_id, user_key');
  }

  const deviceInfo = clients.get(ws);
  if (!deviceInfo || deviceInfo.user_key !== user_key) {
    return error(ws, '未认证或密钥不匹配');
  }

  const userKeyHash = sha256(user_key);

  switch (type) {
    case 'push_snapshots': {
      const { snapshots, sync_token } = rest;
      
      if (!Array.isArray(snapshots)) {
        return error(ws, 'snapshots 必须是数组');
      }

      const results = [];
      for (const encSnapshot of snapshots) {
        try {
          decrypt(encSnapshot.encrypted_data, Buffer.from(user_key, 'hex'));
          
          const result = await db.collection('snapshots').updateOne(
            { user_key: userKeyHash, snapshot_id: encSnapshot.snapshot_id },
            {
              $set: {
                encrypted_data: encSnapshot.encrypted_data,
                data_hash: encSnapshot.data_hash,
                data_size: encSnapshot.data_size,
                updated_at: new Date(),
                last_sync_device: device_id
              },
              $setOnInsert: {
                user_key: userKeyHash,
                snapshot_id: encSnapshot.snapshot_id,
                created_at: new Date()
              }
            },
            { upsert: true }
          );
          
          results.push({
            snapshot_id: encSnapshot.snapshot_id,
            upserted: result.upsertedCount > 0,
            updated: result.modifiedCount > 0
          });
        } catch (e) {
          results.push({
            snapshot_id: encSnapshot.snapshot_id,
            error: '解密验证失败: ' + e.message
          });
        }
      }
      
      send(ws, 'push_ack', { results, sync_token });
      console.log(chalk.blue(`[${device_id}] 推送 ${snapshots.length} 个快照`));
      break;
    }

    case 'pull_snapshots': {
      const { after, limit = 100 } = rest;
      
      const query = { user_key: userKeyHash };
      if (after) {
        query.updated_at = { $gt: new Date(after) };
      }
      
      const cursor = db.collection('snapshots')
        .find(query)
        .sort({ updated_at: -1 })
        .limit(Math.min(limit, 500));
      
      const list = await cursor.toArray();
      
      send(ws, 'pull_ack', {
        snapshots: list.map(s => ({
          snapshot_id: s.snapshot_id,
          encrypted_data: s.encrypted_data,
          data_hash: s.data_hash,
          data_size: s.data_size,
          updated_at: s.updated_at.toISOString(),
          last_sync_device: s.last_sync_device
        })),
        total: list.length
      });
      
      console.log(chalk.blue(`[${device_id}] 拉取 ${list.length} 个快照`));
      break;
    }

    case 'list_snapshot_ids': {
      const list = await db.collection('snapshots')
        .find({ user_key: userKeyHash }, { projection: { snapshot_id: 1, data_hash: 1, updated_at: 1 } })
        .sort({ updated_at: -1 })
        .toArray();
      
      send(ws, 'list_ids_ack', {
        ids: list.map(s => ({
          snapshot_id: s.snapshot_id,
          data_hash: s.data_hash,
          updated_at: s.updated_at.toISOString()
        }))
      });
      break;
    }

    case 'delete_snapshots': {
      const { snapshot_ids } = rest;
      
      if (!Array.isArray(snapshot_ids)) {
        return error(ws, 'snapshot_ids 必须是数组');
      }
      
      const result = await db.collection('snapshots').deleteMany({
        user_key: userKeyHash,
        snapshot_id: { $in: snapshot_ids }
      });
      
      send(ws, 'delete_ack', { deleted_count: result.deletedCount });
      console.log(chalk.yellow(`[${device_id}] 删除 ${result.deletedCount} 个快照`));
      break;
    }

    case 'ping': {
      send(ws, 'pong', { timestamp: Date.now() });
      break;
    }

    default:
      error(ws, `未知消息类型: ${type}`);
  }
}

async function handleAuth(ws, msg) {
  const { type, device_id, user_key, device_name } = JSON.parse(msg);
  
  if (type === 'auth') {
    if (!device_id || !user_key) {
      return error(ws, 'device_id 和 user_key 必填');
    }

    if (process.env.ALLOWED_KEY_PREFIX && !user_key.startsWith(process.env.ALLOWED_KEY_PREFIX)) {
      return error(ws, '密钥格式不正确');
    }

    if (user_key.length !== 64) {
      return error(ws, 'user_key 必须是 64 位十六进制字符串');
    }

    const userKeyHash = sha256(user_key);

    await db.collection('devices').updateOne(
      { user_key: userKeyHash, device_id },
      {
        $set: {
          device_name: device_name || '未知设备',
          last_seen: new Date(),
          last_ip: ws._socket.remoteAddress
        },
        $setOnInsert: {
          user_key: userKeyHash,
          device_id,
          created_at: new Date()
        }
      },
      { upsert: true }
    );

    clients.set(ws, { device_id, user_key, device_name });
    
    const count = await db.collection('snapshots').countDocuments({ user_key: userKeyHash });
    
    send(ws, 'auth_ack', {
      device_id,
      server_time: new Date().toISOString(),
      cloud_snapshot_count: count
    });
    
    console.log(chalk.green(`✓ 设备 [${device_name || device_id}] 已连接，云端快照: ${count}`));
    return true;
  }
  
  return false;
}

async function start() {
  await initMongo();
  
  const wss = new WebSocketServer({ port: PORT });
  
  wss.on('connection', (ws) => {
    console.log(chalk.gray('新客户端连接，等待认证...'));
    
    let authenticated = false;
    
    ws.on('message', async (msg) => {
      try {
        if (!authenticated) {
          authenticated = await handleAuth(ws, msg.toString());
          if (!authenticated) {
            error(ws, '认证失败，请先发送 auth 消息');
          }
          return;
        }
        
        await handleMessage(ws, msg.toString());
      } catch (e) {
        console.error(chalk.red('消息处理错误:'), e);
        error(ws, '服务器内部错误: ' + e.message);
      }
    });
    
    ws.on('close', () => {
      const info = clients.get(ws);
      if (info) {
        console.log(chalk.gray(`设备 [${info.device_name || info.device_id}] 断开连接`));
        clients.delete(ws);
      }
    });
    
    ws.on('error', (e) => {
      console.error(chalk.red('WebSocket 错误:'), e.message);
    });
  });
  
  console.log(chalk.cyan(`
╔══════════════════════════════════════════════════════════╗
║  MemSnapshot 私有云同步服务器已启动                       ║
║  WebSocket: ws://localhost:${PORT}                          ║
║  MongoDB:   ${MONGODB_DB}                                  ║
╚══════════════════════════════════════════════════════════╝
  `));
}

start().catch(err => {
  console.error(chalk.red('启动失败:'), err);
  process.exit(1);
});
