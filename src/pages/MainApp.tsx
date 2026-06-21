import { useEffect, useState } from "react";
import { ProcessList } from "../components/ProcessList";
import { ScanPanel } from "../components/ScanPanel";
import { ScanResults } from "../components/ScanResults";
import { SnapshotList } from "../components/SnapshotList";
import { InjectDialog } from "../components/InjectDialog";
import { ProcessInfo, DataBlock, Snapshot, api, ScanConfig, ProcessCategory } from "../api";
import { useAsync, toast } from "../utils";

type Tab = "scan" | "snapshots";

export function MainApp() {
  const [tab, setTab] = useState<Tab>("scan");
  const [targetProcess, setTargetProcess] = useState<ProcessInfo | null>(null);
  const [categoryFilter, setCategoryFilter] = useState<ProcessCategory[] | null>(["Browser", "IDE", "Design"]);
  const [search, setSearch] = useState("");
  const [scanConfig, setScanConfig] = useState<ScanConfig | null>(null);
  const [scanBlocks, setScanBlocks] = useState<DataBlock[]>([]);
  const [scanning, setScanning] = useState(false);
  const [injectOpen, setInjectOpen] = useState(false);
  const [injectSnapshot, setInjectSnapshot] = useState<Snapshot | null>(null);

  const processesHook = useAsync(() => api.listProcesses(), [], { immediate: true });

  const { data: rawSnapshots, loading: snapsLoading, refresh: refreshSnaps } = useAsync(
    () => api.listSnapshots(null, null, 500, 0),
    [],
    { immediate: true }
  );
  const snapshots = rawSnapshots ?? [];

  useEffect(() => {
    const bind = async () => {
      try {
        await api.listenQuickScan(() => {
          toast("快捷键触发：快速扫描", "info");
          if (targetProcess) {
            doDefaultScan(targetProcess);
          }
        });
      } catch {
        // ignore
      }
    };
    bind();
  }, [targetProcess]);

  const selectProcess = (p: ProcessInfo) => {
    setTargetProcess(p);
    api.setActivePid(p.pid).catch(() => {});
  };

  const doDefaultScan = (p: ProcessInfo) => {
    setScanConfig({
      pid: p.pid,
      patterns: [
        { kind: "Json", value: "" },
        { kind: "Pickle", value: "" },
      ],
      max_region_size_mb: 4,
      region_start: null,
      region_end: null,
    });
    setTab("scan");
    runScan({
      pid: p.pid,
      patterns: [
        { kind: "Json", value: "" },
        { kind: "Pickle", value: "" },
      ],
      max_region_size_mb: 4,
      region_start: null,
      region_end: null,
    });
  };

  const runScan = async (cfg: ScanConfig) => {
    setScanConfig(cfg);
    setScanning(true);
    setScanBlocks([]);
    try {
      const blocks = await api.scanMemory(cfg);
      setScanBlocks(blocks);
      toast(`扫描完成，找到 ${blocks.length} 个数据块`, blocks.length ? "success" : "info");
    } catch (e: any) {
      toast(`扫描失败: ${e}`, "error");
      setScanBlocks([]);
    } finally {
      setScanning(false);
    }
  };

  const openInject = (s: Snapshot) => {
    setInjectSnapshot(s);
    setInjectOpen(true);
  };

  return (
    <div className="h-full flex flex-col min-h-0">
      {/* 顶栏 */}
      <header className="flex items-center justify-between px-4 py-2.5 border-b border-dark-400 bg-dark-200/80 backdrop-blur">
        <div className="flex items-center gap-3">
          <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-accent-cyan to-accent-purple flex items-center justify-center text-dark-300 font-bold">
            M
          </div>
          <div>
            <div className="font-semibold text-accent-cyan">MemSnapshot</div>
            <div className="text-[11px] text-gray-400">进程内存快照 & 注入工具</div>
          </div>
        </div>
        <nav className="flex items-center gap-1 bg-dark-400 p-1 rounded-lg">
          <button
            onClick={() => setTab("scan")}
            className={`px-3 py-1 rounded text-sm transition ${
              tab === "scan"
                ? "bg-accent-cyan text-dark-300 font-medium"
                : "text-gray-300 hover:text-gray-100"
            }`}
          >
            内存扫描
          </button>
          <button
            onClick={() => setTab("snapshots")}
            className={`px-3 py-1 rounded text-sm transition ${
              tab === "snapshots"
                ? "bg-accent-cyan text-dark-300 font-medium"
                : "text-gray-300 hover:text-gray-100"
            }`}
          >
            快照历史
          </button>
        </nav>
        <div className="flex items-center gap-2 text-xs text-gray-400">
          <span className="chip bg-dark-400 border border-gray-700">
            快捷键:
          </span>
          <span className="font-mono">Ctrl+Shift+Alt+M</span>
          <span>呼出面板</span>
          <span className="mx-1">·</span>
          <span className="font-mono">Ctrl+Shift+Alt+S</span>
          <span>快速扫描</span>
        </div>
      </header>

      {/* 主体: 三栏布局 */}
      <div className="flex-1 grid grid-cols-12 gap-3 p-3 min-h-0">
        {/* 左: 进程列表 */}
        <aside className="col-span-3 min-h-0">
          <ProcessList
            processes={processesHook.data ?? []}
            selectedPid={targetProcess?.pid ?? null}
            onSelect={selectProcess}
            loading={processesHook.loading}
            onRefresh={() => processesHook.refresh().catch(() => {})}
            categoryFilter={categoryFilter}
            onCategoryFilter={setCategoryFilter}
            search={search}
            onSearch={setSearch}
          />
        </aside>

        {/* 中右: 内容区 */}
        <section className="col-span-9 flex flex-col gap-3 min-h-0">
          {tab === "scan" ? (
            <>
              <ScanPanel
                pid={targetProcess?.pid ?? null}
                processName={targetProcess?.name ?? ""}
                onScan={runScan}
                scanning={scanning}
              />
              <div className="flex-1 min-h-0">
                <ScanResults
                  blocks={scanBlocks}
                  targetProcess={targetProcess}
                  scanning={scanning}
                  onSnapshotSaved={() => refreshSnaps().catch(() => {})}
                />
              </div>
            </>
          ) : (
            <div className="flex-1 min-h-0">
              <SnapshotList
                snapshots={snapshots}
                processes={processesHook.data ?? []}
                loading={snapsLoading}
                onRefresh={() => refreshSnaps().catch(() => {})}
                onInject={openInject}
              />
            </div>
          )}
        </section>
      </div>

      <InjectDialog
        open={injectOpen}
        snapshot={injectSnapshot}
        processes={processesHook.data ?? []}
        onClose={() => setInjectOpen(false)}
      />
    </div>
  );
}
