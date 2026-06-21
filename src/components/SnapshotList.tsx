import { useState } from "react";
import { Snapshot, api, dataTypeLabel, DataType, ProcessInfo } from "../api";
import { cls, formatTime, toast, copyToClipboard } from "../utils";

interface Props {
  snapshots: Snapshot[];
  processes: ProcessInfo[];
  loading: boolean;
  onRefresh: () => void;
  onInject?: (snapshot: Snapshot) => void;
}

const typeColor: Record<DataType, string> = {
  Json: "bg-green-500/20 text-green-300 border-green-500/30",
  Pickle: "bg-yellow-500/20 text-yellow-300 border-yellow-500/30",
  Base64: "bg-purple-500/20 text-purple-300 border-purple-500/30",
  RegexMatch: "bg-cyan-500/20 text-cyan-300 border-cyan-500/30",
};

export function SnapshotList({ snapshots, processes, loading, onRefresh, onInject }: Props) {
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [search, setSearch] = useState("");
  const [filterPid, setFilterPid] = useState<number | "">("");
  const [editingNote, setEditingNote] = useState<{ id: number; val: string } | null>(null);

  const filtered = snapshots.filter((s) => {
    if (filterPid !== "" && s.pid !== filterPid) return false;
    if (!search.trim()) return true;
    const q = search.toLowerCase();
    return (
      s.process_name.toLowerCase().includes(q) ||
      s.content.toLowerCase().includes(q) ||
      (s.note ?? "").toLowerCase().includes(q)
    );
  });

  const selected = snapshots.find((s) => s.id === selectedId) ?? null;

  const handleDelete = async (id: number) => {
    if (!confirm(`确定要删除快照 #${id} 吗？`)) return;
    try {
      await api.deleteSnapshot(id);
      toast("已删除", "success");
      if (selectedId === id) setSelectedId(null);
      onRefresh();
    } catch (e: any) {
      toast(`删除失败: ${e}`, "error");
    }
  };

  const saveNote = async () => {
    if (!editingNote) return;
    try {
      await api.updateNote(editingNote.id, editingNote.val);
      toast("备注已更新", "success");
      setEditingNote(null);
      onRefresh();
    } catch (e: any) {
      toast(`更新失败: ${e}`, "error");
    }
  };

  return (
    <div className="card p-3 h-full flex flex-col min-h-0">
      <div className="flex items-center justify-between mb-3 gap-2">
        <h3 className="font-semibold text-accent-cyan">
          快照历史
          <span className="ml-2 text-sm font-normal text-gray-400">共 {filtered.length} 条</span>
        </h3>
        <div className="flex items-center gap-2">
          <select
            className="input text-xs py-1"
            value={filterPid}
            onChange={(e) =>
              setFilterPid(e.target.value === "" ? "" : Number(e.target.value))
            }
          >
            <option value="">全部进程</option>
            {processes.map((p) => (
              <option key={p.pid} value={p.pid}>
                {p.name} (PID={p.pid})
              </option>
            ))}
          </select>
          <button className="btn-ghost text-xs py-1" onClick={onRefresh} disabled={loading}>
            {loading ? "..." : "↻ 刷新"}
          </button>
        </div>
      </div>

      <input
        type="text"
        className="input mb-3"
        placeholder="搜索进程名/内容/备注..."
        value={search}
        onChange={(e) => setSearch(e.target.value)}
      />

      <div className="grid grid-cols-5 gap-3 flex-1 min-h-0">
        <div className="col-span-2 overflow-auto pr-1 min-h-0 space-y-1.5">
          {loading && (
            <div className="text-center text-gray-400 text-sm py-6">加载中...</div>
          )}
          {!loading && filtered.length === 0 && (
            <div className="text-center text-gray-500 text-sm py-8">暂无快照</div>
          )}
          {filtered.map((s) => (
            <button
              key={s.id}
              onClick={() => setSelectedId(s.id)}
              className={cls(
                "w-full text-left rounded px-2.5 py-2 border transition",
                selectedId === s.id
                  ? "bg-accent-cyan/10 border-accent-cyan/50"
                  : "bg-dark-400/50 border-transparent hover:border-gray-600"
              )}
            >
              <div className="flex items-center justify-between gap-2">
                <span className="font-medium text-sm">#{s.id}</span>
                <span className={cls("chip border text-[10px]", typeColor[s.data_type])}>
                  {dataTypeLabel[s.data_type]}
                </span>
              </div>
              <div className="text-xs text-gray-300 mt-1 truncate">
                {s.process_name} · PID={s.pid}
              </div>
              <div className="text-[11px] font-mono text-accent-purple mt-0.5">
                0x{s.address.toString(16).toUpperCase()} · {s.size}B
              </div>
              <div className="text-[11px] text-gray-500 mt-0.5">
                {formatTime(s.created_at)}
              </div>
              {s.note && (
                <div className="text-[11px] text-accent-yellow mt-1 line-clamp-1">
                  📝 {s.note}
                </div>
              )}
            </button>
          ))}
        </div>

        <div className="col-span-3 border-l border-dark-400 pl-3 flex flex-col min-h-0">
          {!selected ? (
            <div className="text-gray-500 text-sm text-center py-12 flex-1">
              选择左侧快照查看详情
            </div>
          ) : (
            <div className="flex flex-col gap-3 flex-1 min-h-0">
              <div className="flex flex-wrap gap-2 items-center">
                <span className={cls("chip border", typeColor[selected.data_type])}>
                  {dataTypeLabel[selected.data_type]}
                </span>
                <span className="chip bg-dark-400 border border-gray-700">
                  {selected.process_name} · PID={selected.pid}
                </span>
                <span className="chip bg-dark-400 border border-gray-700 font-mono">
                  0x{selected.address.toString(16).toUpperCase()}
                </span>
                <span className="chip bg-dark-400 border border-gray-700">
                  {selected.size} bytes
                </span>
                <span className="chip bg-dark-400 border border-gray-700">
                  {formatTime(selected.created_at)}
                </span>
                <button
                  className="btn-ghost text-xs ml-auto"
                  onClick={() => copyToClipboard(selected.content)}
                >
                  复制内容
                </button>
              </div>

              {/* 备注编辑 */}
              <div className="flex gap-2">
                {editingNote && editingNote.id === selected.id ? (
                  <>
                    <input
                      type="text"
                      className="input flex-1"
                      value={editingNote.val}
                      onChange={(e) =>
                        setEditingNote({ id: selected.id, val: e.target.value })
                      }
                      placeholder="输入备注..."
                    />
                    <button className="btn-primary text-xs" onClick={saveNote}>
                      保存
                    </button>
                    <button
                      className="btn-ghost text-xs"
                      onClick={() => setEditingNote(null)}
                    >
                      取消
                    </button>
                  </>
                ) : (
                  <>
                    <input
                      type="text"
                      className="input flex-1 text-xs"
                      value={selected.note ?? ""}
                      placeholder="点击右侧按钮编辑备注"
                      disabled
                    />
                    <button
                      className="btn-ghost text-xs"
                      onClick={() =>
                        setEditingNote({ id: selected.id, val: selected.note ?? "" })
                      }
                    >
                      编辑备注
                    </button>
                  </>
                )}
              </div>

              {/* 操作按钮 */}
              <div className="flex gap-2">
                <button
                  className="btn-success flex-1"
                  onClick={() => onInject?.(selected)}
                >
                  🚀 注入到进程
                </button>
                <button className="btn-danger" onClick={() => handleDelete(selected.id)}>
                  删除
                </button>
              </div>

              {/* 内容预览 */}
              <div className="rounded border border-dark-400 bg-dark-300/70 p-3 overflow-auto flex-1 min-h-0">
                <pre className="text-xs font-mono whitespace-pre-wrap break-all text-gray-200">
                  {selected.content}
                </pre>
              </div>

              <details className="text-xs">
                <summary className="cursor-pointer text-gray-400 hover:text-gray-300 py-1">
                  原始 HEX 数据
                </summary>
                <pre className="mt-2 p-2 rounded bg-dark-400 font-mono break-all text-[11px] text-accent-yellow/90 max-h-48 overflow-auto">
                  {selected.raw_data}
                </pre>
              </details>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
