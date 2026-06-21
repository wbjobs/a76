import { useEffect, useState } from "react";
import {
  ProcessInfo,
  Snapshot,
  api,
  DataBlock,
  ScanConfig,
  ProcessCategory,
  dataTypeLabel,
  DataType,
} from "../api";
import { useAsync, toast, formatTime, cls } from "../utils";
import { InjectDialog } from "../components/InjectDialog";

const typeColor: Record<DataType, string> = {
  Json: "bg-green-500/20 text-green-300 border-green-500/30",
  Pickle: "bg-yellow-500/20 text-yellow-300 border-yellow-500/30",
  Base64: "bg-purple-500/20 text-purple-300 border-purple-500/30",
  RegexMatch: "bg-cyan-500/20 text-cyan-300 border-cyan-500/30",
};

export function FloatingPanel() {
  const [activePid, setActivePid] = useState<number | null>(null);
  const [tab, setTab] = useState<"quick" | "snapshots">("quick");
  const [filter, setFilter] = useState<"all" | ProcessCategory>("all");
  const [q, setQ] = useState("");
  const [scanning, setScanning] = useState(false);
  const [blocks, setBlocks] = useState<DataBlock[]>([]);
  const [selectedBlock, setSelectedBlock] = useState<DataBlock | null>(null);
  const [injectOpen, setInjectOpen] = useState(false);
  const [injectSnapshot, setInjectSnapshot] = useState<Snapshot | null>(null);

  const procHook = useAsync(() => api.listProcesses(), [], { immediate: true });
  const snapsHook = useAsync(
    () => api.listSnapshots(null, null, 200, 0),
    [],
    { immediate: true }
  );

  useEffect(() => {
    api.getActivePid().then((p) => setActivePid(p)).catch(() => {});
  }, []);

  useEffect(() => {
    const bind = async () => {
      try {
        await api.listenQuickScan(() => {
          const target = procHook.data?.find(
            (p) =>
              p.pid === activePid ||
              (procHook.data && procHook.data[0] ? procHook.data[0].pid : null)
          );
          const p =
            target ??
            procHook.data?.find((x) => x.category !== "Other") ??
            procHook.data?.[0];
          if (p) {
            setActivePid(p.pid);
            doScan(p.pid);
          }
        });
      } catch {}
    };
    bind();
  }, [procHook.data, activePid]);

  const processes = procHook.data ?? [];
  const filteredProcs = processes
    .filter((p) => (filter === "all" ? true : p.category === filter))
    .filter((p) =>
      !q.trim()
        ? true
        : p.name.toLowerCase().includes(q.toLowerCase()) ||
          p.pid.toString().includes(q)
    )
    .sort((a, b) => {
      const ai = Number(a.pid === activePid);
      const bi = Number(b.pid === activePid);
      if (ai !== bi) return bi - ai;
      return b.memory_mb - a.memory_mb;
    })
    .slice(0, 40);

  const snapshots = snapsHook.data ?? [];

  const doScan = async (pid: number) => {
    setScanning(true);
    setBlocks([]);
    setSelectedBlock(null);
    setActivePid(pid);
    await api.setActivePid(pid).catch(() => {});
    const cfg: ScanConfig = {
      pid,
      patterns: [
        { kind: "Json", value: "" },
        { kind: "Pickle", value: "" },
      ],
      max_region_size_mb: 4,
      region_start: null,
      region_end: null,
    };
    try {
      const res = await api.scanMemory(cfg);
      setBlocks(res.slice(0, 200));
      toast(`快速扫描完成，${res.length} 个块`, "success");
    } catch (e: any) {
      toast(`扫描失败: ${e}`, "error");
    } finally {
      setScanning(false);
    }
  };

  const saveSnapshot = async (b: DataBlock) => {
    const proc = processes.find((p) => p.pid === activePid);
    if (!proc) {
      toast("无目标进程", "error");
      return;
    }
    try {
      const id = await api.autoSnapshot(
        proc.pid,
        proc.name,
        b.address,
        b.size,
        b.data_type,
        null
      );
      toast(`快照已保存 #${id}`, "success");
      snapsHook.refresh().catch(() => {});
    } catch (e: any) {
      toast(`保存失败: ${e}`, "error");
    }
  };

  const quickInject = async (s: Snapshot) => {
    setInjectSnapshot(s);
    setInjectOpen(true);
  };

  const closePanel = () => {
    api.hideFloatingPanel().catch(() => {});
  };

  return (
    <div className="h-full w-full bg-dark-300/95 backdrop-blur rounded-xl border border-dark-400 shadow-2xl flex flex-col overflow-hidden">
      {/* 拖动手柄 & 关闭按钮 */}
      <div
        className="flex items-center justify-between px-3 py-2 border-b border-dark-400 bg-dark-200/80 select-none"
        data-tauri-drag-region
      >
        <div className="flex items-center gap-2">
          <div className="flex gap-1.5">
            <span className="w-3 h-3 rounded-full bg-accent-red/80"></span>
            <span className="w-3 h-3 rounded-full bg-accent-yellow/80"></span>
            <span className="w-3 h-3 rounded-full bg-accent-green/80"></span>
          </div>
          <div className="ml-2 font-semibold text-sm text-accent-cyan">
            MemSnapshot 浮动面板
          </div>
        </div>
        <button
          onClick={closePanel}
          className="text-gray-400 hover:text-accent-red text-lg leading-none w-6 h-6 flex items-center justify-center"
        >
          ×
        </button>
      </div>

      {/* Tabs */}
      <div className="flex items-center gap-1 px-3 py-2 border-b border-dark-400 bg-dark-200/50">
        <button
          onClick={() => setTab("quick")}
          className={`px-3 py-1 rounded text-xs transition ${
            tab === "quick"
              ? "bg-accent-cyan text-dark-300 font-medium"
              : "text-gray-300 hover:bg-dark-400"
          }`}
        >
          ⚡ 快速扫描
        </button>
        <button
          onClick={() => setTab("snapshots")}
          className={`px-3 py-1 rounded text-xs transition ${
            tab === "snapshots"
              ? "bg-accent-cyan text-dark-300 font-medium"
              : "text-gray-300 hover:bg-dark-400"
          }`}
        >
          🗂 快照 ({snapshots.length})
        </button>
        <div className="ml-auto text-[11px] text-gray-500">
          快捷键: <span className="font-mono text-gray-400">Ctrl+Shift+Alt+M</span>
        </div>
      </div>

      {/* 主体 */}
      {tab === "quick" ? (
        <div className="grid grid-cols-5 gap-0 flex-1 min-h-0">
          {/* 进程 */}
          <div className="col-span-2 border-r border-dark-400 flex flex-col min-h-0">
            <div className="p-2 border-b border-dark-400 space-y-2">
              <div className="flex gap-1 flex-wrap">
                {(["all", "Browser", "IDE", "Design", "Other"] as const).map((c) => (
                  <button
                    key={c}
                    onClick={() => setFilter(c)}
                    className={cls(
                      "chip border text-[10px] transition",
                      filter === c
                        ? "bg-accent-cyan/20 text-accent-cyan border-accent-cyan/50"
                        : "bg-dark-400 text-gray-400 border-gray-700 hover:text-gray-200"
                    )}
                  >
                    {c === "all" ? "全部" : c}
                  </button>
                ))}
              </div>
              <input
                type="text"
                className="input text-xs py-1 w-full"
                placeholder="搜索进程..."
                value={q}
                onChange={(e) => setQ(e.target.value)}
              />
            </div>
            <div className="flex-1 overflow-auto min-h-0 p-1 space-y-1">
              {filteredProcs.length === 0 ? (
                <div className="text-xs text-gray-500 text-center py-4">无进程</div>
              ) : (
                filteredProcs.map((p) => (
                  <button
                    key={p.pid}
                    onClick={() => doScan(p.pid)}
                    className={cls(
                      "w-full text-left text-xs rounded px-2 py-1.5 border transition",
                      p.pid === activePid
                        ? "bg-accent-cyan/10 border-accent-cyan/50"
                        : "bg-dark-400/40 border-transparent hover:border-gray-600"
                    )}
                  >
                    <div className="flex items-center justify-between gap-2">
                      <span className="truncate font-medium">{p.name}</span>
                      <span className="text-[10px] text-gray-400">{p.memory_mb.toFixed(0)}M</span>
                    </div>
                    <div className="text-[10px] text-gray-500 mt-0.5 flex items-center justify-between">
                      <span>PID={p.pid}</span>
                      <span className={cls("chip border text-[9px]", typeColorForCat(p.category))}>
                        {p.category}
                      </span>
                    </div>
                  </button>
                ))
              )}
            </div>
          </div>

          {/* 扫描结果 */}
          <div className="col-span-3 flex flex-col min-h-0">
            <div className="px-3 py-2 border-b border-dark-400 text-xs flex items-center justify-between">
              <div>
                {activePid ? (
                  <span>
                    目标:{" "}
                    <span className="text-accent-cyan">
                      {processes.find((p) => p.pid === activePid)?.name ?? "?"} (PID={activePid})
                    </span>
                  </span>
                ) : (
                  <span className="text-gray-500">请选择进程开始扫描</span>
                )}
              </div>
              <div className="text-gray-400">
                {scanning ? (
                  <span>
                    <span className="inline-block w-3 h-3 rounded-full border-2 border-accent-cyan border-t-transparent animate-spin mr-1 align-middle"></span>
                    扫描中...
                  </span>
                ) : (
                  `共 ${blocks.length} 个数据块`
                )}
              </div>
            </div>

            <div className="flex-1 overflow-auto min-h-0 p-2 space-y-1.5">
              {blocks.length === 0 && !scanning && (
                <div className="text-xs text-gray-500 text-center py-10">
                  点击左侧进程开始快速扫描 (JSON + Pickle)
                </div>
              )}
              {blocks.slice(0, 100).map((b, i) => (
                <div
                  key={`${b.address}-${i}`}
                  onClick={() =>
                    setSelectedBlock(
                      selectedBlock?.address === b.address &&
                      selectedBlock?.size === b.size
                        ? null
                        : b
                    )
                  }
                  className="rounded border border-dark-400 bg-dark-400/50 hover:border-gray-600 p-2 cursor-pointer transition"
                >
                  <div className="flex items-center gap-2 mb-1">
                    <span className={cls("chip border text-[10px]", typeColor[b.data_type])}>
                      {dataTypeLabel[b.data_type]}
                    </span>
                    <span className="font-mono text-[10px] text-accent-purple">
                      0x{b.address.toString(16).toUpperCase()}
                    </span>
                    <span className="text-[10px] text-gray-400 ml-auto">{b.size}B</span>
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        saveSnapshot(b);
                      }}
                      className="btn-primary !py-0.5 !px-2 !text-[10px]"
                      disabled={!activePid}
                    >
                      保存
                    </button>
                  </div>
                  <div className="text-[11px] text-gray-300 line-clamp-2 break-all">
                    {b.content.slice(0, 200).replace(/\s+/g, " ")}
                  </div>
                  {selectedBlock?.address === b.address && (
                    <pre className="mt-2 p-2 rounded bg-dark-300 text-[10px] font-mono break-all text-gray-200 max-h-40 overflow-auto">
                      {b.content}
                    </pre>
                  )}
                </div>
              ))}
            </div>
          </div>
        </div>
      ) : (
        <div className="flex-1 overflow-auto min-h-0 p-2 space-y-1.5">
          {snapshots.length === 0 ? (
            <div className="text-xs text-gray-500 text-center py-10">暂无快照记录</div>
          ) : (
            snapshots.slice(0, 100).map((s) => (
              <div
                key={s.id}
                className="rounded border border-dark-400 bg-dark-400/50 p-2 hover:border-gray-600 transition"
              >
                <div className="flex items-center gap-2 mb-1">
                  <span className="font-semibold text-xs text-accent-cyan">#{s.id}</span>
                  <span className={cls("chip border text-[10px]", typeColor[s.data_type])}>
                    {dataTypeLabel[s.data_type]}
                  </span>
                  <span className="text-[10px] text-gray-300 ml-auto">
                    {s.process_name} · {s.pid}
                  </span>
                </div>
                <div className="text-[10px] text-gray-500 mb-1 flex justify-between">
                  <span className="font-mono text-accent-purple">
                    0x{s.address.toString(16).toUpperCase()} · {s.size}B
                  </span>
                  <span>{formatTime(s.created_at)}</span>
                </div>
                {s.note && (
                  <div className="text-[11px] text-accent-yellow mb-1">📝 {s.note}</div>
                )}
                <div className="text-[11px] text-gray-300 line-clamp-2 break-all">
                  {s.content.slice(0, 200).replace(/\s+/g, " ")}
                </div>
                <div className="mt-2 flex gap-1 justify-end">
                  <button
                    onClick={() => quickInject(s)}
                    className="btn-success !py-0.5 !px-2 !text-[10px]"
                  >
                    🚀 注入
                  </button>
                </div>
              </div>
            ))
          )}
        </div>
      )}

      <InjectDialog
        open={injectOpen}
        snapshot={injectSnapshot}
        processes={processes}
        onClose={() => setInjectOpen(false)}
      />
    </div>
  );
}

function typeColorForCat(c: ProcessCategory): string {
  switch (c) {
    case "Browser":
      return "bg-blue-500/20 text-blue-300 border-blue-500/30";
    case "IDE":
      return "bg-purple-500/20 text-purple-300 border-purple-500/30";
    case "Design":
      return "bg-pink-500/20 text-pink-300 border-pink-500/30";
    default:
      return "bg-gray-500/20 text-gray-300 border-gray-500/30";
  }
}
