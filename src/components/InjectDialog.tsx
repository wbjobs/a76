import { useEffect, useState } from "react";
import { Snapshot, ProcessInfo, api, InjectionResult, InjectionStep, InjectionLog } from "../api";
import { cls, toast, formatTime } from "../utils";

interface Props {
  open: boolean;
  snapshot: Snapshot | null;
  processes: ProcessInfo[];
  onClose: () => void;
  onDone?: (r: InjectionResult) => void;
}

export function InjectDialog({ open, snapshot, processes, onClose, onDone }: Props) {
  const [pid, setPid] = useState<number | "">("");
  const [addr, setAddr] = useState("");
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<InjectionResult | null>(null);
  const [recentLogs, setRecentLogs] = useState<InjectionLog[]>([]);

  useEffect(() => {
    if (open && snapshot) {
      setPid(snapshot.pid);
      setAddr("0x" + snapshot.address.toString(16).toUpperCase());
      setResult(null);
      loadLogs();
    }
  }, [open, snapshot]);

  const loadLogs = async () => {
    try {
      const logs = await api.listInjectionLogs(null, snapshot?.id ?? null, 10, 0);
      setRecentLogs(logs);
    } catch {}
  };

  if (!open || !snapshot) return null;

  const target = processes.find((p) => p.pid === pid) ?? null;

  const parseAddr = () => {
    const s = addr.trim();
    if (!s) return snapshot.address;
    const v = s.startsWith("0x") || s.startsWith("0X") ? parseInt(s.slice(2), 16) : parseInt(s, 10);
    return isNaN(v) ? snapshot.address : v;
  };

  const run = async () => {
    if (!pid) {
      toast("请选择目标进程", "error");
      return;
    }
    setLoading(true);
    setResult(null);
    try {
      const res = await api.injectSnapshot(snapshot.id, Number(pid), parseAddr());
      setResult(res);
      if (res.success) {
        toast(res.message, "success");
      } else {
        toast(res.message, "error");
      }
      onDone?.(res);
      loadLogs();
    } catch (e: any) {
      toast(`注入失败: ${e}`, "error");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="card p-5 w-[680px] max-w-[92vw] max-h-[90vh] overflow-auto">
        <div className="flex items-start justify-between mb-4">
          <div>
            <h3 className="font-semibold text-lg text-accent-cyan">
              �️ 安全注入
            </h3>
            <p className="text-xs text-gray-400 mt-1">
              VirtualAllocEx → WriteProcessMemory → shellcode(RtlMoveMemory) → CreateRemoteThread
            </p>
          </div>
          <button
            onClick={onClose}
            className="text-gray-400 hover:text-gray-200 text-xl leading-none"
          >
            ×
          </button>
        </div>

        <div className="space-y-3 mb-4 p-3 bg-dark-300 rounded border border-dark-400">
          <div className="grid grid-cols-2 gap-3 text-xs">
            <div>
              <div className="text-gray-400">快照</div>
              <div className="text-gray-100 font-mono">#{snapshot.id}</div>
            </div>
            <div>
              <div className="text-gray-400">原进程</div>
              <div className="text-gray-100">
                {snapshot.process_name} (PID={snapshot.pid})
              </div>
            </div>
            <div>
              <div className="text-gray-400">原地址</div>
              <div className="text-accent-purple font-mono">
                0x{snapshot.address.toString(16).toUpperCase()}
              </div>
            </div>
            <div>
              <div className="text-gray-400">大小</div>
              <div className="text-gray-100">{snapshot.size} bytes</div>
            </div>
          </div>
        </div>

        <div className="space-y-3">
          <div>
            <label className="text-xs text-gray-400 block mb-1">
              目标进程 <span className="text-accent-red">*</span>
            </label>
            <select
              className="input w-full"
              value={pid}
              onChange={(e) => setPid(e.target.value === "" ? "" : Number(e.target.value))}
            >
              <option value="">-- 请选择进程 --</option>
              {processes.map((p) => (
                <option key={p.pid} value={p.pid}>
                  [{p.category}] {p.name} (PID={p.pid}, {p.memory_mb.toFixed(1)} MB)
                </option>
              ))}
            </select>
            {pid && !target && (
              <div className="text-[11px] text-accent-yellow mt-1">
                ⚠ PID={pid} 不在运行中
              </div>
            )}
          </div>

          <div>
            <label className="text-xs text-gray-400 block mb-1">
              目标内存地址 (HEX / DEC)
            </label>
            <input
              type="text"
              className="input w-full font-mono"
              value={addr}
              onChange={(e) => setAddr(e.target.value)}
              placeholder="0x..."
            />
          </div>

          <div className="text-[11px] bg-accent-cyan/10 border border-accent-cyan/30 rounded p-2.5 text-accent-cyan space-y-1.5">
            <div className="font-semibold">🛡️ 安全注入流程</div>
            <ol className="list-decimal list-inside space-y-0.5 text-gray-300">
              <li>校验目标地址是否在可写内存区域</li>
              <li>ReadProcessMemory 备份目标原始数据（用于回滚恢复）</li>
              <li>VirtualAllocEx 在目标进程分配临时 RW 内存页</li>
              <li>WriteProcessMemory 将快照数据写入临时区</li>
              <li>解析 ntdll!RtlMoveMemory 地址，构建 x64 shellcode</li>
              <li>WriteProcessMemory 将 shellcode 写入临时区末尾</li>
              <li>CreateRemoteThread 执行 shellcode（调用 RtlMoveMemory 拷贝数据）</li>
              <li>WaitForSingleObject 等待远程线程完成</li>
              <li>ReadProcessMemory 验证写入数据匹配率</li>
              <li>VirtualFreeEx 释放临时区，清理痕迹</li>
            </ol>
            <div className="text-accent-yellow mt-1">
              ⚠️ 任何步骤失败将自动回滚：释放临时区 + 恢复备份原始数据
            </div>
          </div>
        </div>

        {result && (
          <div className="mt-4 space-y-3">
            <div
              className={cls(
                "p-3 rounded border text-xs",
                result.success
                  ? "bg-accent-green/10 border-accent-green/30 text-accent-green"
                  : "bg-accent-red/10 border-accent-red/30 text-accent-red"
              )}
            >
              <div className="font-semibold mb-1 text-sm">
                {result.success ? "✓ 注入成功" : "✗ 注入失败"}
                {result.rolled_back && (
                  <span className="ml-2 text-accent-yellow font-normal">（已自动回滚）</span>
                )}
              </div>
              <div>地址: 0x{result.address.toString(16).toUpperCase()}</div>
              <div>已写入: {result.bytes_written} / {snapshot.size} bytes</div>
              {result.temp_alloc_address && (
                <div>
                  临时区: 0x{result.temp_alloc_address.toString(16).toUpperCase()}（已释放）
                </div>
              )}
              {result.memcpy_result != null && (
                <div>远程线程退出码: {result.memcpy_result}</div>
              )}
              {result.log_id && (
                <div>日志ID: #{result.log_id}</div>
              )}
              <div className="mt-1">{result.message}</div>
            </div>

            {/* 注入步骤详情 */}
            {result.steps.length > 0 && (
              <div className="rounded border border-dark-400 overflow-hidden">
                <div className="bg-dark-400 px-3 py-1.5 text-xs font-semibold text-gray-300">
                  注入步骤详情 ({result.steps.length} 步)
                </div>
                <div className="max-h-64 overflow-auto">
                  <table className="w-full text-[11px]">
                    <thead>
                      <tr className="bg-dark-400 text-gray-400">
                        <th className="px-2 py-1 text-left">状态</th>
                        <th className="px-2 py-1 text-left">步骤</th>
                        <th className="px-2 py-1 text-left">地址</th>
                        <th className="px-2 py-1 text-left">大小</th>
                        <th className="px-2 py-1 text-left">返回值 / 错误</th>
                        <th className="px-2 py-1 text-left">时间</th>
                      </tr>
                    </thead>
                    <tbody>
                      {result.steps.map((s, i) => (
                        <tr
                          key={i}
                          className={cls(
                            "border-t border-dark-400",
                            s.success ? "text-gray-200" : "text-accent-red"
                          )}
                        >
                          <td className="px-2 py-1">
                            {s.success ? "✓" : "✗"}
                          </td>
                          <td className="px-2 py-1 font-medium">{s.step}</td>
                          <td className="px-2 py-1 font-mono">
                            {s.address != null
                              ? `0x${s.address.toString(16).toUpperCase()}`
                              : "-"}
                          </td>
                          <td className="px-2 py-1">
                            {s.size != null ? `${s.size}B` : "-"}
                          </td>
                          <td className="px-2 py-1 max-w-[200px] truncate">
                            {s.error ? (
                              <span className="text-accent-red">{s.error}</span>
                            ) : (
                              s.return_value
                            )}
                          </td>
                          <td className="px-2 py-1 text-gray-400 whitespace-nowrap">
                            {s.timestamp.split("T")[1]?.split(".")[0] ?? s.timestamp}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            )}
          </div>
        )}

        {/* 历史注入日志 */}
        {recentLogs.length > 0 && (
          <div className="mt-4">
            <div className="text-xs font-semibold text-gray-400 mb-2">
              该快照的历史注入记录 ({recentLogs.length})
            </div>
            <div className="max-h-32 overflow-auto space-y-1">
              {recentLogs.map((log) => (
                <div
                  key={log.id}
                  className={cls(
                    "flex items-center gap-2 px-2 py-1 rounded text-[11px] border",
                    log.success
                      ? "bg-accent-green/5 border-accent-green/20"
                      : "bg-accent-red/5 border-accent-red/20"
                  )}
                >
                  <span className={log.success ? "text-accent-green" : "text-accent-red"}>
                    {log.success ? "✓" : "✗"}
                  </span>
                  <span className="text-gray-300">#{log.id}</span>
                  <span className="text-gray-400">
                    PID={log.target_pid} → 0x{log.target_address.toString(16).toUpperCase()}
                  </span>
                  <span className="text-gray-400">{log.data_size}B</span>
                  {log.rolled_back && (
                    <span className="text-accent-yellow">已回滚</span>
                  )}
                  {log.temp_alloc_address && (
                    <span className="text-gray-500">
                      临时区=0x{log.temp_alloc_address.toString(16).toUpperCase()}
                    </span>
                  )}
                  <span className="ml-auto text-gray-500">
                    {formatTime(log.created_at)}
                  </span>
                </div>
              ))}
            </div>
          </div>
        )}

        <div className="flex items-center justify-end gap-2 mt-5 pt-3 border-t border-dark-400">
          <button className="btn-ghost" onClick={onClose}>
            {result ? "关闭" : "取消"}
          </button>
          <button className="btn-success" onClick={run} disabled={loading || !pid}>
            {loading ? "注入中..." : "🛡️ 安全注入"}
          </button>
        </div>
      </div>
    </div>
  );
}
