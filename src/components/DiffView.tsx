import { useState, useMemo } from "react";
import { api, Snapshot, DiffResult, dataTypeLabel } from "../api";
import { useAsync, toastApi } from "../utils";

interface DiffViewProps {
  snapshots: Snapshot[];
}

export default function DiffView({ snapshots }: DiffViewProps) {
  const [oldId, setOldId] = useState<number | null>(null);
  const [newId, setNewId] = useState<number | null>(null);
  const [diffResult, setDiffResult] = useState<DiffResult | null>(null);
  const [viewMode, setViewMode] = useState<"inline" | "split">("inline");
  const [loading, setLoading] = useState(false);

  const oldSnap = useMemo(() => snapshots.find((s) => s.id === oldId), [snapshots, oldId]);
  const newSnap = useMemo(() => snapshots.find((s) => s.id === newId), [snapshots, newId]);

  const canCompare = oldId !== null && newId !== null && oldId !== newId;

  const handleCompare = async () => {
    if (oldId === null || newId === null) return;
    setLoading(true);
    try {
      const result = await api.diffSnapshots(oldId, newId);
      setDiffResult(result);
      toastApi.success(`对比完成，${result.changed_bytes} 字节变化`);
    } catch (e: any) {
      toastApi.error("对比失败: " + e);
    } finally {
      setLoading(false);
    }
  };

  const renderHexWithHighlight = (hex: string, highlights: Array<{ start: number; end: number; kind: "insert" | "delete" | "modify" }>) => {
    const chars = hex.split("");
    const result: JSX.Element[] = [];
    let currentKind: string | null = null;
    let currentText = "";

    for (let i = 0; i < chars.length; i += 2) {
      const byteIndex = i / 2;
      const highlight = highlights.find((h) => byteIndex >= h.start && byteIndex < h.end);
      const kind = highlight?.kind || null;

      if (kind !== currentKind) {
        if (currentText) {
          const className = currentKind === "insert"
            ? "bg-green-200 text-green-800"
            : currentKind === "delete"
            ? "bg-red-200 text-red-800"
            : currentKind === "modify"
            ? "bg-yellow-200 text-yellow-800"
            : "";
          result.push(
            <span key={result.length} className={className}>
              {currentText}
            </span>
          );
          currentText = "";
        }
        currentKind = kind;
      }

      currentText += chars[i] + (chars[i + 1] || "");
      if ((i / 2 + 1) % 16 === 0) {
        currentText += "\n";
      } else {
        currentText += " ";
      }
    }

    if (currentText) {
      const className = currentKind === "insert"
        ? "bg-green-200 text-green-800"
        : currentKind === "delete"
        ? "bg-red-200 text-red-800"
        : currentKind === "modify"
        ? "bg-yellow-200 text-yellow-800"
        : "";
      result.push(
        <span key={result.length} className={className}>
          {currentText}
        </span>
      );
    }

    return result;
  };

  const formatBytes = (n: number) => {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  };

  const similarityColor = (sim: number) => {
    if (sim >= 0.9) return "text-green-600";
    if (sim >= 0.7) return "text-yellow-600";
    if (sim >= 0.5) return "text-orange-600";
    return "text-red-600";
  };

  const ChunkLegend = () => (
    <div className="flex gap-4 text-sm">
      <div className="flex items-center gap-1">
        <span className="w-3 h-3 bg-green-200 rounded"></span>
        <span className="text-gray-600">插入</span>
      </div>
      <div className="flex items-center gap-1">
        <span className="w-3 h-3 bg-red-200 rounded"></span>
        <span className="text-gray-600">删除</span>
      </div>
      <div className="flex items-center gap-1">
        <span className="w-3 h-3 bg-yellow-200 rounded"></span>
        <span className="text-gray-600">修改</span>
      </div>
    </div>
  );

  return (
    <div className="p-6 space-y-6 h-full overflow-y-auto">
      <div className="flex items-center justify-between">
        <h2 className="text-xl font-bold text-gray-800">📊 快照差异对比</h2>
        {diffResult && <ChunkLegend />}
      </div>

      <div className="bg-white rounded-lg border border-gray-200 p-6 space-y-4">
        <h3 className="text-lg font-semibold text-gray-800">选择快照</h3>

        <div className="grid grid-cols-2 gap-6">
          <div className="space-y-2">
            <label className="block text-sm font-medium text-gray-700">
              🔴 旧版本 (Before)
            </label>
            <select
              value={oldId || ""}
              onChange={(e) => {
                setOldId(e.target.value ? Number(e.target.value) : null);
                setDiffResult(null);
              }}
              className="w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-2 focus:ring-blue-500"
            >
              <option value="">选择快照...</option>
              {snapshots.map((s) => (
                <option key={s.id} value={s.id}>
                  #{s.id} {s.process_name} @ 0x{s.address.toString(16)} - {new Date(s.created_at).toLocaleString()}
                </option>
              ))}
            </select>
            {oldSnap && (
              <div className="text-sm text-gray-600 bg-red-50 p-3 rounded border border-red-100">
                <p><strong>进程:</strong> {oldSnap.process_name} (PID: {oldSnap.pid})</p>
                <p><strong>地址:</strong> 0x{oldSnap.address.toString(16)}</p>
                <p><strong>大小:</strong> {formatBytes(oldSnap.size)}</p>
                <p><strong>类型:</strong> {dataTypeLabel[oldSnap.data_type]}</p>
                <p><strong>时间:</strong> {new Date(oldSnap.created_at).toLocaleString()}</p>
              </div>
            )}
          </div>

          <div className="space-y-2">
            <label className="block text-sm font-medium text-gray-700">
              🟢 新版本 (After)
            </label>
            <select
              value={newId || ""}
              onChange={(e) => {
                setNewId(e.target.value ? Number(e.target.value) : null);
                setDiffResult(null);
              }}
              className="w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-2 focus:ring-blue-500"
            >
              <option value="">选择快照...</option>
              {snapshots.map((s) => (
                <option key={s.id} value={s.id}>
                  #{s.id} {s.process_name} @ 0x{s.address.toString(16)} - {new Date(s.created_at).toLocaleString()}
                </option>
              ))}
            </select>
            {newSnap && (
              <div className="text-sm text-gray-600 bg-green-50 p-3 rounded border border-green-100">
                <p><strong>进程:</strong> {newSnap.process_name} (PID: {newSnap.pid})</p>
                <p><strong>地址:</strong> 0x{newSnap.address.toString(16)}</p>
                <p><strong>大小:</strong> {formatBytes(newSnap.size)}</p>
                <p><strong>类型:</strong> {dataTypeLabel[newSnap.data_type]}</p>
                <p><strong>时间:</strong> {new Date(newSnap.created_at).toLocaleString()}</p>
              </div>
            )}
          </div>
        </div>

        <div className="flex gap-3 pt-2">
          <button
            onClick={handleCompare}
            disabled={!canCompare || loading}
            className="px-6 py-2 bg-gradient-to-r from-purple-500 to-indigo-600 text-white rounded-md hover:from-purple-600 hover:to-indigo-700 disabled:opacity-50 font-medium"
          >
            {loading ? "对比中..." : "🔍 开始对比"}
          </button>
          {diffResult && (
            <div className="flex gap-3 ml-auto items-center">
              <div className="flex bg-gray-100 rounded-md p-0.5">
                <button
                  onClick={() => setViewMode("inline")}
                  className={`px-3 py-1 rounded text-sm ${viewMode === "inline" ? "bg-white shadow" : ""}`}
                >
                  并列视图
                </button>
                <button
                  onClick={() => setViewMode("split")}
                  className={`px-3 py-1 rounded text-sm ${viewMode === "split" ? "bg-white shadow" : ""}`}
                >
                  对比视图
                </button>
              </div>
              <button
                onClick={() => setDiffResult(null)}
                className="px-3 py-1 text-gray-600 hover:text-gray-800 text-sm"
              >
                清除结果
              </button>
            </div>
          )}
        </div>
      </div>

      {diffResult && (
        <>
          <div className="bg-white rounded-lg border border-gray-200 p-6">
            <h3 className="text-lg font-semibold text-gray-800 mb-4">📈 对比结果</h3>
            <div className="grid grid-cols-4 gap-4 mb-4">
              <div className="bg-gradient-to-br from-red-50 to-red-100 rounded-lg p-4 border border-red-200">
                <div className="text-xs text-red-600 mb-1">旧版本大小</div>
                <div className="text-2xl font-bold text-red-700">{formatBytes(diffResult.old_size)}</div>
              </div>
              <div className="bg-gradient-to-br from-green-50 to-green-100 rounded-lg p-4 border border-green-200">
                <div className="text-xs text-green-600 mb-1">新版本大小</div>
                <div className="text-2xl font-bold text-green-700">{formatBytes(diffResult.new_size)}</div>
              </div>
              <div className="bg-gradient-to-br from-orange-50 to-orange-100 rounded-lg p-4 border border-orange-200">
                <div className="text-xs text-orange-600 mb-1">变化字节数</div>
                <div className="text-2xl font-bold text-orange-700">{formatBytes(diffResult.changed_bytes)}</div>
              </div>
              <div className="bg-gradient-to-br from-blue-50 to-blue-100 rounded-lg p-4 border border-blue-200">
                <div className="text-xs text-blue-600 mb-1">相似度</div>
                <div className={`text-2xl font-bold ${similarityColor(diffResult.similarity)}`}>
                  {(diffResult.similarity * 100).toFixed(1)}%
                </div>
              </div>
            </div>

            {diffResult.chunks.length > 0 && (
              <div className="mb-4">
                <h4 className="font-medium text-gray-700 mb-2">变化区块 ({diffResult.chunks.length} 个)</h4>
                <div className="max-h-48 overflow-y-auto border border-gray-200 rounded-md">
                  <table className="w-full text-sm">
                    <thead className="bg-gray-50 sticky top-0">
                      <tr>
                        <th className="px-3 py-2 text-left text-gray-600">类型</th>
                        <th className="px-3 py-2 text-left text-gray-600">偏移(旧)</th>
                        <th className="px-3 py-2 text-left text-gray-600">偏移(新)</th>
                        <th className="px-3 py-2 text-left text-gray-600">长度</th>
                      </tr>
                    </thead>
                    <tbody>
                      {diffResult.chunks.map((chunk, idx) => (
                        <tr key={idx} className="border-t border-gray-100 hover:bg-gray-50">
                          <td className="px-3 py-2">
                            <span
                              className={`px-2 py-0.5 rounded text-xs font-medium ${
                                chunk.kind === "insert"
                                  ? "bg-green-100 text-green-700"
                                  : chunk.kind === "delete"
                                  ? "bg-red-100 text-red-700"
                                  : "bg-yellow-100 text-yellow-700"
                              }`}
                            >
                              {chunk.kind === "insert" ? "+ 插入" : chunk.kind === "delete" ? "- 删除" : "~ 修改"}
                            </span>
                          </td>
                          <td className="px-3 py-2 font-mono text-gray-600">
                            0x{chunk.old_start.toString(16)} - 0x{chunk.old_end.toString(16)}
                          </td>
                          <td className="px-3 py-2 font-mono text-gray-600">
                            0x{chunk.new_start.toString(16)} - 0x{chunk.new_end.toString(16)}
                          </td>
                          <td className="px-3 py-2 text-gray-600">
                            {chunk.kind === "delete"
                              ? chunk.old_end - chunk.old_start
                              : chunk.new_end - chunk.new_start}{" "}
                            bytes
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            )}
          </div>

          {viewMode === "inline" && (
            <div className="bg-white rounded-lg border border-gray-200 p-6">
              <h3 className="text-lg font-semibold text-gray-800 mb-4">📝 详细差异 (新版本)</h3>
              <div className="bg-gray-900 rounded-lg p-4 overflow-x-auto">
                <pre className="text-green-400 font-mono text-sm whitespace-pre-wrap">
                  {renderHexWithHighlight(diffResult.new_preview,
                    diffResult.chunks.map((c) => ({
                      start: c.new_start,
                      end: c.new_end,
                      kind: c.kind,
                    }))
                  )}
                </pre>
              </div>
            </div>
          )}

          {viewMode === "split" && (
            <div className="grid grid-cols-2 gap-6">
              <div className="bg-white rounded-lg border border-gray-200 p-6">
                <h3 className="text-lg font-semibold text-gray-800 mb-4">🔴 旧版本</h3>
                <div className="bg-gray-900 rounded-lg p-4 overflow-x-auto max-h-96">
                  <pre className="text-red-400 font-mono text-sm whitespace-pre-wrap">
                    {renderHexWithHighlight(diffResult.old_preview,
                      diffResult.chunks
                        .filter((c) => c.kind === "delete" || c.kind === "modify")
                        .map((c) => ({
                          start: c.old_start,
                          end: c.old_end,
                          kind: c.kind,
                        }))
                    )}
                  </pre>
                </div>
              </div>
              <div className="bg-white rounded-lg border border-gray-200 p-6">
                <h3 className="text-lg font-semibold text-gray-800 mb-4">🟢 新版本</h3>
                <div className="bg-gray-900 rounded-lg p-4 overflow-x-auto max-h-96">
                  <pre className="text-green-400 font-mono text-sm whitespace-pre-wrap">
                    {renderHexWithHighlight(diffResult.new_preview,
                      diffResult.chunks
                        .filter((c) => c.kind === "insert" || c.kind === "modify")
                        .map((c) => ({
                          start: c.new_start,
                          end: c.new_end,
                          kind: c.kind,
                        }))
                    )}
                  </pre>
                </div>
              </div>
            </div>
          )}

          {oldSnap?.content && newSnap?.content && oldSnap.data_type === newSnap.data_type && (
            <div className="bg-white rounded-lg border border-gray-200 p-6">
              <h3 className="text-lg font-semibold text-gray-800 mb-4">📄 内容对比 ({dataTypeLabel[oldSnap.data_type]})</h3>
              <div className="grid grid-cols-2 gap-6">
                <div>
                  <div className="text-sm font-medium text-gray-600 mb-2">🔴 旧内容</div>
                  <pre className="bg-gray-50 border border-gray-200 rounded p-3 text-sm overflow-x-auto max-h-64 whitespace-pre-wrap">
                    {oldSnap.content}
                  </pre>
                </div>
                <div>
                  <div className="text-sm font-medium text-gray-600 mb-2">🟢 新内容</div>
                  <pre className="bg-gray-50 border border-gray-200 rounded p-3 text-sm overflow-x-auto max-h-64 whitespace-pre-wrap">
                    {newSnap.content}
                  </pre>
                </div>
              </div>
            </div>
          )}
        </>
      )}

      {snapshots.length === 0 && (
        <div className="bg-gray-50 rounded-lg border-2 border-dashed border-gray-300 p-12 text-center">
          <div className="text-4xl mb-4">📭</div>
          <div className="text-gray-500">暂无快照，请先扫描并保存快照</div>
        </div>
      )}

      {snapshots.length < 2 && snapshots.length > 0 && (
        <div className="bg-yellow-50 rounded-lg border border-yellow-200 p-6 text-center">
          <div className="text-4xl mb-4">📊</div>
          <div className="text-yellow-700">至少需要 2 个快照才能进行对比，请先保存更多快照</div>
        </div>
      )}
    </div>
  );
}
