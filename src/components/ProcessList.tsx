import { ProcessInfo, categoryLabel, ProcessCategory } from "../api";
import { cls } from "../utils";

interface Props {
  processes: ProcessInfo[];
  selectedPid: number | null;
  onSelect: (p: ProcessInfo) => void;
  loading?: boolean;
  onRefresh?: () => void;
  categoryFilter?: ProcessCategory[] | null;
  onCategoryFilter?: (c: ProcessCategory[] | null) => void;
  search?: string;
  onSearch?: (s: string) => void;
}

const categoryColor: Record<ProcessCategory, string> = {
  Browser: "bg-blue-500/20 text-blue-300 border-blue-500/30",
  IDE: "bg-purple-500/20 text-purple-300 border-purple-500/30",
  Design: "bg-pink-500/20 text-pink-300 border-pink-500/30",
  Other: "bg-gray-500/20 text-gray-300 border-gray-500/30",
};

export function ProcessList(props: Props) {
  const {
    processes,
    selectedPid,
    onSelect,
    loading,
    onRefresh,
    categoryFilter,
    onCategoryFilter,
    search = "",
    onSearch,
  } = props;

  const allCats: ProcessCategory[] = ["Browser", "IDE", "Design", "Other"];
  const toggleCat = (c: ProcessCategory) => {
    const set = new Set(categoryFilter ?? []);
    if (set.has(c)) set.delete(c);
    else set.add(c);
    onCategoryFilter?.(set.size ? Array.from(set) : null);
  };

  const filtered = processes
    .filter(
      (p) =>
        (!categoryFilter || categoryFilter.length === 0 || categoryFilter.includes(p.category))
    )
    .filter((p) =>
      !search
        ? true
        : p.name.toLowerCase().includes(search.toLowerCase()) ||
          p.pid.toString().includes(search)
    )
    .sort((a, b) => {
      const ca = Number(a.category !== "Other");
      const cb = Number(b.category !== "Other");
      if (ca !== cb) return cb - ca;
      return b.memory_mb - a.memory_mb;
    });

  return (
    <div className="card p-3 h-full flex flex-col min-h-0">
      <div className="flex items-center justify-between gap-2 mb-3">
        <h3 className="font-semibold text-accent-cyan">进程列表</h3>
        <button className="btn-ghost text-xs" onClick={onRefresh} disabled={loading}>
          {loading ? "刷新中..." : "↻ 刷新"}
        </button>
      </div>

      <div className="flex flex-wrap gap-2 mb-3">
        {allCats.map((c) => {
          const active = categoryFilter?.includes(c);
          return (
            <button
              key={c}
              onClick={() => toggleCat(c)}
              className={cls(
                "chip border transition",
                active ? categoryColor[c] : "bg-dark-400 text-gray-400 border-gray-700"
              )}
            >
              {categoryLabel[c]}
            </button>
          );
        })}
      </div>

      <input
        type="text"
        className="input mb-3"
        placeholder="搜索进程名或 PID..."
        value={search}
        onChange={(e) => onSearch?.(e.target.value)}
      />

      <div className="flex-1 overflow-auto min-h-0 space-y-1.5">
        {filtered.length === 0 ? (
          <div className="text-sm text-gray-500 text-center py-8">
            {loading ? "加载进程中..." : "无匹配进程"}
          </div>
        ) : (
          filtered.map((p) => (
            <button
              key={p.pid}
              onClick={() => onSelect(p)}
              className={cls(
                "w-full text-left rounded px-3 py-2 border transition",
                selectedPid === p.pid
                  ? "bg-accent-cyan/10 border-accent-cyan/50"
                  : "bg-dark-400/50 border-transparent hover:border-gray-600"
              )}
            >
              <div className="flex items-center justify-between gap-2">
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="truncate font-medium">{p.name}</span>
                    <span className={cls("chip border", categoryColor[p.category])}>
                      {categoryLabel[p.category]}
                    </span>
                  </div>
                  <div className="text-xs text-gray-400 mt-0.5">
                    PID: <span className="text-gray-300">{p.pid}</span>
                    <span className="mx-1.5">·</span>
                    {p.memory_mb.toFixed(1)} MB
                  </div>
                </div>
              </div>
            </button>
          ))
        )}
      </div>
    </div>
  );
}
