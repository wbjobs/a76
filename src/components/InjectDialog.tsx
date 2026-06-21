import { useEffect, useState } from "react";
import { Snapshot, ProcessInfo, api, InjectionResult } from "../api";
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

  useEffect(() => {
    if (open && snapshot) {
      setPid(snapshot.pid);
      setAddr("0x" + snapshot.address.toString(16).toUpperCase());
      setResult(null);
    }
  }, [open, snapshot]);

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
    } catch (e: any) {
      toast(`注入失败: ${e}`, "error");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="card p-5 w-[560px] max-w-[90vw] max-h-[85vh] overflow-auto">
        <div className="flex items-start justify-between mb-4">
          <div>
            <h3 className="font-semibold text-lg text-accent-cyan">
              🚀 内存注入
            </h3>
            <p className="text-xs text-gray-400 mt-1">
              将快照 #{snapshot.id} 的内容写回目标进程的指定内存地址
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
            <div className="col-span-2">
              <div className="text-gray-400">创建时间</div>
              <div className="text-gray-100">{formatTime(snapshot.created_at)}</div>
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
            {target && (
              <div className="text-[11px] text-gray-400 mt-1">
                目标进程内存: {target.memory_mb.toFixed(1)} MB · 路径: {target.path ?? "未知"}
              </div>
            )}
            {pid && !target && (
              <div className="text-[11px] text-accent-yellow mt-1">
                ⚠ 当前进程 PID={pid} 不在运行中，建议先刷新进程列表
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
            <div className="text-[11px] text-gray-400 mt-1">
              留空或默认会使用快照原始地址:{" "}
              <span className="font-mono text-accent-purple">
                0x{snapshot.address.toString(16).toUpperCase()}
              </span>
            </div>
          </div>

          <div className="text-[11px] bg-accent-yellow/10 border border-accent-yellow/30 rounded p-2 text-accent-yellow">
            ⚠️ <strong>安全提示：</strong>
            错误的地址或大小写入会导致目标进程崩溃（AccessViolation / SegFault）。
            系统会自动检查目标地址是否属于可写内存区域，但仍建议在测试环境操作。
          </div>
        </div>

        {result && (
          <div
            className={cls(
              "mt-4 p-3 rounded border text-xs",
              result.success
                ? "bg-accent-green/10 border-accent-green/30 text-accent-green"
                : "bg-accent-red/10 border-accent-red/30 text-accent-red"
            )}
          >
            <div className="font-semibold mb-1">
              {result.success ? "✓ 注入成功" : "✗ 注入失败"}
            </div>
            <div>地址: 0x{result.address.toString(16).toUpperCase()}</div>
            <div>已写入: {result.bytes_written} / {snapshot.size} bytes</div>
            <div className="mt-1">{result.message}</div>
          </div>
        )}

        <div className="flex items-center justify-end gap-2 mt-5 pt-3 border-t border-dark-400">
          <button className="btn-ghost" onClick={onClose}>
            {result ? "关闭" : "取消"}
          </button>
          <button className="btn-success" onClick={run} disabled={loading || !pid}>
            {loading ? "注入中..." : "确认注入"}
          </button>
        </div>
      </div>
    </div>
  );
}
