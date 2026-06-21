import { useState } from "react";
import { DataBlock, ProcessInfo, api, dataTypeLabel, DataType } from "../api";
import { toast, cls, copyToClipboard } from "../utils";

interface Props {
  blocks: DataBlock[];
  targetProcess: ProcessInfo | null;
  scanning: boolean;
  onSnapshotSaved?: (snapshotId: number) => void;
}

const typeColor: Record<DataType, string> = {
  Json: "bg-green-500/20 text-green-300 border-green-500/30",
  Pickle: "bg-yellow-500/20 text-yellow-300 border-yellow-500/30",
  Base64: "bg-purple-500/20 text-purple-300 border-purple-500/30",
  RegexMatch: "bg-cyan-500/20 text-cyan-300 border-cyan-500/30",
};

export function ScanResults({ blocks, targetProcess, scanning, onSnapshotSaved }: Props) {
  const [selected, setSelected] = useState<DataBlock | null>(null);
  const [saving, setSaving] = useState<number | null>(null);
  const [note, setNote] = useState("");

  const saveAsSnapshot = async (b: DataBlock) => {
    if (!targetProcess) {
      toast("请先选择进程", "error");
      return;
    }
    setSaving(b.address);
    try {
      const id = await api.autoSnapshot(
        targetProcess.pid,
        targetProcess.name,
        b.address,
        b.size,
        b.data_type,
        note || null
      );
      toast(`快照已保存 #${id}`, "success");
      onSnapshotSaved?.(id);
    } catch (e: any) {
      toast(`保存失败: ${e ?? "未知错误"}`, "error");
    } finally {
      setSaving(null);
    }
  };

  const previewContent = (b: DataBlock) => {
    if (b.data_type === "Json") return b.content;
    if (b.data_type === "RegexMatch") return b.content;
    if (b.data_type === "Base64") return b.content;
    return b.content;
  };

  return (
    <div className="card p-3 h-full flex flex-col min-h-0">
      <div className="flex items-center justify-between mb-3">
        <h3 className="font-semibold text-accent-cyan">
          扫描结果
          <span className="ml-2 text-sm font-normal text-gray-400">
            共 {blocks.length} 个数据块
          </span>
        </h3>
      </div>

      <div className="grid grid-cols-5 gap-3 flex-1 min-h-0">
        {/* 左列表 */}
        <div className="col-span-2 overflow-auto space-y-1.5 pr-1 min-h-0">
          {scanning && (
            <div className="text-sm text-gray-400 text-center py-6">
              <div className="inline-block animate-spin rounded-full border-2 border-accent-cyan border-t-transparent w-5 h-5 mr-2 align-middle"></div>
              扫描中，请稍候...
            </div>
          )}
          {!scanning && blocks.length === 0 && (
            <div className="text-sm text-gray-500 text-center py-8">
              暂无数据，请先运行扫描
            </div>
          )}
          {blocks.map((b, i) => (
            <button
              key={`${b.address}-${i}`}
              onClick={() => setSelected(b)}
              className={cls(
                "w-full text-left rounded px-2.5 py-2 border transition text-left",
                selected?.address === b.address
                  ? "bg-accent-cyan/10 border-accent-cyan/50"
                  : "bg-dark-400/50 border-transparent hover:border-gray-600"
              )}
            >
              <div className="flex items-center justify-between gap-2">
                <span className={cls("chip border text-[10px]", typeColor[b.data_type])}>
                  {dataTypeLabel[b.data_type]}
                </span>
                <span className="text-xs text-gray-400 font-mono">{b.size} B</span>
              </div>
              <div className="text-xs font-mono text-accent-purple mt-1">
                0x{b.address.toString(16).toUpperCase()}
              </div>
              <div className="text-xs text-gray-400 mt-0.5 truncate">
                {b.content.slice(0, 80).replace(/\s+/g, " ")}
              </div>
            </button>
          ))}
        </div>

        {/* 右详情 */}
        <div className="col-span-3 border-l border-dark-400 pl-3 flex flex-col min-h-0">
          {!selected ? (
            <div className="text-sm text-gray-500 text-center py-12 flex-1">
              选择左侧数据块查看详情
            </div>
          ) : (
            <div className="flex-1 flex flex-col min-h-0 gap-3">
              <div className="flex flex-wrap gap-2 items-center">
                <span className={cls("chip border", typeColor[selected.data_type])}>
                  {dataTypeLabel[selected.data_type]}
                </span>
                <span className="chip bg-dark-400 border border-gray-700">
                  地址:{" "}
                  <span className="font-mono text-accent-purple">
                    0x{selected.address.toString(16).toUpperCase()}
                  </span>
                </span>
                <span className="chip bg-dark-400 border border-gray-700">
                  大小: {selected.size} bytes
                </span>
                <button
                  className="btn-ghost text-xs ml-auto"
                  onClick={() =>
                    copyToClipboard(
                      `地址: 0x${selected.address.toString(16)}\n${previewContent(selected)}`
                    )
                  }
                >
                  复制内容
                </button>
              </div>

              <div className="grid grid-cols-4 gap-2 items-center">
                <input
                  type="text"
                  placeholder="添加备注 (可选)"
                  className="input col-span-3"
                  value={note}
                  onChange={(e) => setNote(e.target.value)}
                />
                <button
                  className={cls(
                    "btn-success col-span-1",
                    saving === selected.address && "opacity-60"
                  )}
                  onClick={() => saveAsSnapshot(selected)}
                  disabled={saving === selected.address || !targetProcess}
                >
                  {saving === selected.address ? "保存中..." : "保存快照"}
                </button>
              </div>

              <div className="rounded border border-dark-400 bg-dark-300/70 p-3 overflow-auto flex-1 min-h-0">
                <pre className="text-xs font-mono whitespace-pre-wrap break-all text-gray-200">
                  {previewContent(selected)}
                </pre>
              </div>

              <details className="text-xs">
                <summary className="cursor-pointer text-gray-400 hover:text-gray-300 py-1">
                  查看原始 HEX (前 512 字节)
                </summary>
                <pre className="mt-2 p-2 rounded bg-dark-400 font-mono break-all text-[11px] text-accent-yellow/90">
                  {selected.raw_hex}
                </pre>
              </details>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
