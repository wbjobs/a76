import { useState } from "react";
import { ScanConfig, ScanPattern, PatternKind } from "../api";
import { cls } from "../utils";

interface Props {
  pid: number | null;
  processName: string;
  onScan: (cfg: ScanConfig) => void;
  scanning: boolean;
}

const kindOpts: { label: string; value: PatternKind }[] = [
  { label: "JSON 对象/数组", value: "Json" },
  { label: "Python Pickle 序列化", value: "Pickle" },
  { label: "Base64 编码块", value: "Base64" },
  { label: "正则表达式", value: "Regex" },
];

export function ScanPanel({ pid, processName, onScan, scanning }: Props) {
  const [detect, setDetect] = useState<Record<PatternKind, boolean>>({
    Json: true,
    Pickle: true,
    Base64: false,
    Regex: false,
  });
  const [regex, setRegex] = useState('"(token|session|user|auth|config)"\\s*:');
  const [maxSize, setMaxSize] = useState(4);
  const [startAddr, setStartAddr] = useState("");
  const [endAddr, setEndAddr] = useState("");

  const buildConfig = (): ScanConfig | null => {
    if (!pid) return null;
    const patterns: ScanPattern[] = [];
    (Object.keys(detect) as PatternKind[]).forEach((k) => {
      if (!detect[k]) return;
      if (k === "Regex") {
        if (!regex.trim()) return;
        patterns.push({ kind: "Regex", value: regex.trim() });
      } else {
        patterns.push({ kind: k, value: "" });
      }
    });
    if (patterns.length === 0) return null;
    const parseHex = (s: string) => {
      const v = s.trim();
      if (!v) return null;
      if (v.startsWith("0x") || v.startsWith("0X")) {
        return parseInt(v.slice(2), 16);
      }
      return parseInt(v, 10);
    };
    return {
      pid,
      patterns,
      max_region_size_mb: maxSize,
      region_start: parseHex(startAddr),
      region_end: parseHex(endAddr),
    };
  };

  const doScan = () => {
    const cfg = buildConfig();
    if (cfg) onScan(cfg);
  };

  const disabled = !pid || scanning;

  return (
    <div className="card p-4 space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="font-semibold text-accent-cyan">内存扫描配置</h3>
        <div className="text-xs text-gray-400">
          {pid ? (
            <span>
              目标: <span className="text-gray-200">{processName}</span>
              <span className="mx-1">·</span>
              PID={pid}
            </span>
          ) : (
            "请先选择目标进程"
          )}
        </div>
      </div>

      <div>
        <div className="text-xs text-gray-400 mb-2">检测的数据格式</div>
        <div className="grid grid-cols-2 gap-2">
          {kindOpts.map((o) => (
            <label
              key={o.value}
              className={cls(
                "flex items-center gap-2 px-3 py-2 rounded border cursor-pointer text-sm transition",
                detect[o.value]
                  ? "bg-accent-cyan/10 border-accent-cyan/50 text-accent-cyan"
                  : "bg-dark-400 border-gray-700 hover:border-gray-500"
              )}
            >
              <input
                type="checkbox"
                className="accent-cyan"
                checked={detect[o.value]}
                onChange={(e) => setDetect({ ...detect, [o.value]: e.target.checked })}
              />
              {o.label}
            </label>
          ))}
        </div>
      </div>

      {detect.Regex && (
        <div>
          <label className="text-xs text-gray-400 block mb-1">正则表达式</label>
          <input
            type="text"
            className="input w-full font-mono"
            value={regex}
            onChange={(e) => setRegex(e.target.value)}
            placeholder='例如: "(token|secret)"\s*:'
          />
        </div>
      )}

      <div className="grid grid-cols-3 gap-3">
        <div>
          <label className="text-xs text-gray-400 block mb-1">单区域上限 (MB)</label>
          <input
            type="number"
            className="input w-full"
            min={1}
            value={maxSize}
            onChange={(e) => setMaxSize(Number(e.target.value) || 1)}
          />
        </div>
        <div>
          <label className="text-xs text-gray-400 block mb-1">起始地址</label>
          <input
            type="text"
            className="input w-full font-mono"
            placeholder="0x0 (可选)"
            value={startAddr}
            onChange={(e) => setStartAddr(e.target.value)}
          />
        </div>
        <div>
          <label className="text-xs text-gray-400 block mb-1">结束地址</label>
          <input
            type="text"
            className="input w-full font-mono"
            placeholder="0x... (可选)"
            value={endAddr}
            onChange={(e) => setEndAddr(e.target.value)}
          />
        </div>
      </div>

      <div className="flex items-center gap-2 pt-1">
        <button className="btn-primary" onClick={doScan} disabled={disabled}>
          {scanning ? "扫描中..." : "开始扫描"}
        </button>
        <div className="text-xs text-gray-500">
          提示：扫描可能需要几秒到几十秒，取决于目标进程内存大小
        </div>
      </div>
    </div>
  );
}
