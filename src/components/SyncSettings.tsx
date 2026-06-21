import { useState, useEffect } from "react";
import { api, SyncConfigInfo, SyncStatus } from "../api";
import { useAsync, toastApi } from "../utils";

export default function SyncSettings() {
  const [config, setConfig] = useState<SyncConfigInfo | null>(null);
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [serverAddress, setServerAddress] = useState("ws://localhost:8080");
  const [password, setPassword] = useState("");
  const [deviceName, setDeviceName] = useState("");
  const [autoSync, setAutoSync] = useState(false);
  const [showPassword, setShowPassword] = useState(false);
  const [operationLoading, setOperationLoading] = useState<string | null>(null);

  const { data: configData, loading: configLoading, refresh: refreshConfig } = useAsync(api.getSyncConfig, [], {
    onError: (e) => toastApi.error("加载配置失败: " + e),
  });

  const { data: statusData, loading: statusLoading, refresh: refreshStatus } = useAsync(api.getSyncStatus, [], {
    onError: () => {},
  });

  useEffect(() => {
    if (configData) {
      setConfig(configData);
      setServerAddress(configData.server_address);
      setDeviceName(configData.device_name || "");
      setAutoSync(configData.auto_sync);
    }
  }, [configData]);

  useEffect(() => {
    if (statusData) setStatus(statusData);
  }, [statusData]);

  const runOperation = async (name: string, fn: () => Promise<void>) => {
    setOperationLoading(name);
    try {
      await fn();
    } catch (e: any) {
      toastApi.error("操作失败: " + e);
    } finally {
      setOperationLoading(null);
    }
  };

  const handleSave = () => {
    if (!serverAddress) return toastApi.error("请输入服务器地址");
    if (!password && !config?.has_key) return toastApi.error("请输入加密密码");

    runOperation("save", async () => {
      const result = await api.setSyncConfig({
        server_address: serverAddress,
        encryption_password: password,
        device_name: deviceName || null,
        auto_sync: autoSync,
      });
      setConfig(result);
      setPassword("");
      toastApi.success("同步配置已保存");
      await refreshConfig();
    });
  };

  const handleConnect = () => {
    runOperation("connect", async () => {
      const s = await api.connectSync();
      setStatus(s);
      toastApi.success("已连接到同步服务器");
      await refreshStatus();
    });
  };

  const handleDisconnect = () => {
    runOperation("disconnect", async () => {
      const s = await api.disconnectSync();
      setStatus(s);
      toastApi.info("已断开同步连接");
      await refreshStatus();
    });
  };

  const handlePushAll = () => {
    runOperation("push", async () => {
      const r = await api.syncPushAll();
      toastApi.success(`推送成功: 已同步 ${r.pushed} 个快照`);
      await refreshStatus();
    });
  };

  const handlePullAll = () => {
    runOperation("pull", async () => {
      const r = await api.syncPullAll();
      toastApi.success(`拉取成功: 新增 ${r.pulled} 个快照`);
      await refreshStatus();
    });
  };

  const handleClear = () => {
    if (!confirm("确定要清除同步配置吗？")) return;
    runOperation("clear", async () => {
      await api.clearSyncConfig();
      setConfig(null);
      setStatus(null);
      setPassword("");
      toastApi.info("同步配置已清除");
      await refreshConfig();
    });
  };

  const handleGenerateKey = async () => {
    const key = await api.generateEncryptionKey();
    setPassword(key);
    setShowPassword(true);
  };

  const isLoading = configLoading || statusLoading || operationLoading !== null;

  return (
    <div className="p-6 space-y-6 h-full overflow-y-auto">
      <div className="flex items-center justify-between">
        <h2 className="text-xl font-bold text-gray-800">☁️ 私有云同步设置</h2>
        {status && (
          <div className={`px-3 py-1 rounded-full text-sm font-medium ${status.connected ? "bg-green-100 text-green-700" : "bg-gray-100 text-gray-600"}`}>
            {status.connected ? "● 已连接" : "○ 未连接"}
          </div>
        )}
      </div>

      <div className="bg-gradient-to-r from-blue-50 to-indigo-50 rounded-lg p-4 border border-blue-200">
        <h3 className="font-medium text-blue-800 mb-2">🔐 安全提示</h3>
        <ul className="text-sm text-blue-700 space-y-1">
          <li>• 快照数据使用 AES-256-GCM 端到端加密</li>
          <li>• 服务器只存储加密数据，无法解密读取</li>
          <li>• 请记住加密密码，丢失后无法恢复数据</li>
          <li>• 多台设备需使用相同的密码才能同步</li>
        </ul>
      </div>

      <div className="bg-white rounded-lg border border-gray-200 p-6 space-y-4">
        <h3 className="text-lg font-semibold text-gray-800">连接配置</h3>

        <div>
          <label className="block text-sm font-medium text-gray-700 mb-1">服务器地址</label>
          <input
            type="text"
            value={serverAddress}
            onChange={(e) => setServerAddress(e.target.value)}
            placeholder="ws://your-server.com:8080"
            className="w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-2 focus:ring-blue-500 focus:border-transparent"
            disabled={isLoading}
          />
        </div>

        <div>
          <label className="block text-sm font-medium text-gray-700 mb-1">设备名称</label>
          <input
            type="text"
            value={deviceName}
            onChange={(e) => setDeviceName(e.target.value)}
            placeholder="办公室电脑 / 家里电脑"
            className="w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-2 focus:ring-blue-500 focus:border-transparent"
            disabled={isLoading}
          />
        </div>

        <div>
          <label className="block text-sm font-medium text-gray-700 mb-1">加密密码</label>
          <div className="flex gap-2">
            <input
              type={showPassword ? "text" : "password"}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder={config?.has_key ? "•••••••• (已保存)" : "输入加密密码"}
              className="flex-1 px-3 py-2 border border-gray-300 rounded-md focus:ring-2 focus:ring-blue-500 focus:border-transparent"
              disabled={isLoading}
            />
            <button
              onClick={() => setShowPassword(!showPassword)}
              className="px-3 py-2 text-gray-600 hover:text-gray-800 border border-gray-300 rounded-md"
              disabled={isLoading}
            >
              {showPassword ? "🙈" : "👁️"}
            </button>
            <button
              onClick={handleGenerateKey}
              className="px-3 py-2 text-blue-600 hover:text-blue-800 border border-blue-300 rounded-md text-sm"
              disabled={isLoading}
            >
              生成强密钥
            </button>
          </div>
          {config?.has_key && (
            <p className="text-xs text-gray-500 mt-1">留空则使用已保存的密钥</p>
          )}
        </div>

        <div className="flex items-center gap-2">
          <input
            type="checkbox"
            id="autoSync"
            checked={autoSync}
            onChange={(e) => setAutoSync(e.target.checked)}
            className="w-4 h-4 text-blue-600 rounded"
          />
          <label htmlFor="autoSync" className="text-sm text-gray-700">自动同步（暂未实现）</label>
        </div>

        {config && (
          <div className="bg-gray-50 rounded-md p-3 text-sm">
            <p className="text-gray-600"><span className="font-medium">设备ID:</span> <code className="bg-gray-200 px-1 rounded">{config.device_id}</code></p>
            {config.last_sync && (
              <p className="text-gray-600 mt-1"><span className="font-medium">上次同步:</span> {new Date(config.last_sync).toLocaleString()}</p>
            )}
          </div>
        )}

        <div className="flex gap-2 pt-2">
          <button
            onClick={handleSave}
            disabled={isLoading}
            className="px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:opacity-50 font-medium"
          >
            💾 保存配置
          </button>
          <button
            onClick={handleConnect}
            disabled={isLoading || !config || status?.connected}
            className="px-4 py-2 bg-green-600 text-white rounded-md hover:bg-green-700 disabled:opacity-50 font-medium"
          >
            {operationLoading === "connect" ? "连接中..." : "🔌 连接"}
          </button>
          <button
            onClick={handleDisconnect}
            disabled={isLoading || !status?.connected}
            className="px-4 py-2 bg-yellow-600 text-white rounded-md hover:bg-yellow-700 disabled:opacity-50 font-medium"
          >
            {operationLoading === "disconnect" ? "断开中..." : "⏹️ 断开"}
          </button>
          {config && (
            <button
              onClick={handleClear}
              disabled={isLoading}
              className="px-4 py-2 bg-red-100 text-red-700 rounded-md hover:bg-red-200 disabled:opacity-50 font-medium ml-auto"
            >
              🗑️ 清除配置
            </button>
          )}
        </div>
      </div>

      {status?.connected && (
        <div className="bg-white rounded-lg border border-gray-200 p-6 space-y-4">
          <h3 className="text-lg font-semibold text-gray-800">手动同步</h3>
          
          <div className="grid grid-cols-2 gap-4">
            <div className="bg-blue-50 rounded-lg p-4 border border-blue-200">
              <div className="text-2xl font-bold text-blue-600">{status.cloud_snapshot_count}</div>
              <div className="text-sm text-blue-700">云端快照总数</div>
            </div>
            <div className="bg-purple-50 rounded-lg p-4 border border-purple-200">
              <div className="text-2xl font-bold text-purple-600">{status.last_sync ? new Date(status.last_sync).toLocaleDateString() : "从未"}</div>
              <div className="text-sm text-purple-700">上次同步时间</div>
            </div>
          </div>

          <div className="flex gap-3">
            <button
              onClick={handlePushAll}
              disabled={isLoading}
              className="flex-1 px-4 py-3 bg-gradient-to-r from-blue-500 to-blue-600 text-white rounded-lg hover:from-blue-600 hover:to-blue-700 disabled:opacity-50 font-medium flex items-center justify-center gap-2"
            >
              <span>⬆️</span> 
              {operationLoading === "push" ? "推送中..." : "推送所有快照到云端"}
            </button>
            <button
              onClick={handlePullAll}
              disabled={isLoading}
              className="flex-1 px-4 py-3 bg-gradient-to-r from-green-500 to-green-600 text-white rounded-lg hover:from-green-600 hover:to-green-700 disabled:opacity-50 font-medium flex items-center justify-center gap-2"
            >
              <span>⬇️</span>
              {operationLoading === "pull" ? "拉取中..." : "从云端拉取所有快照"}
            </button>
          </div>
        </div>
      )}

      <div className="bg-gray-50 rounded-lg p-4 border border-gray-200">
        <h3 className="font-medium text-gray-800 mb-2">📖 部署说明</h3>
        <div className="text-sm text-gray-600 space-y-1 font-mono bg-white p-3 rounded border">
          <p># 1. 启动 MongoDB</p>
          <p>$ mongod</p>
          <p className="mt-2"># 2. 启动同步服务器</p>
          <p>$ cd sync-server && npm install && npm start</p>
          <p className="mt-2"># 3. 配置客户端</p>
          <p>服务器地址: ws://localhost:8080</p>
        </div>
      </div>
    </div>
  );
}
