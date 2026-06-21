import { invoke } from "@tauri-apps/api/tauri";
import { event } from "@tauri-apps/api";

export type ProcessCategory = "Browser" | "IDE" | "Design" | "Other";

export interface ProcessInfo {
  pid: number;
  name: string;
  path?: string | null;
  memory_mb: number;
  category: ProcessCategory;
}

export interface MemoryRegion {
  base_address: number;
  size: number;
  protection: string;
  is_readable: boolean;
  is_writable: boolean;
}

export type DataType = "Json" | "Pickle" | "Base64" | "RegexMatch";
export type PatternKind = "Json" | "Pickle" | "Base64" | "Regex";

export interface DataBlock {
  address: number;
  size: number;
  data_type: DataType;
  content: string;
  raw_hex: string;
}

export interface ScanPattern {
  kind: PatternKind;
  value: string;
}

export interface ScanConfig {
  pid: number;
  patterns: ScanPattern[];
  max_region_size_mb?: number | null;
  region_start?: number | null;
  region_end?: number | null;
}

export interface Snapshot {
  id: number;
  process_name: string;
  pid: number;
  address: number;
  size: number;
  data_type: DataType;
  content: string;
  raw_data: string;
  created_at: string;
  note?: string | null;
}

export interface SnapshotCreateParams {
  process_name: string;
  pid: number;
  address: number;
  size: number;
  data_type: DataType;
  content: string;
  raw_data: string;
  note?: string | null;
}

export interface InjectionResult {
  success: boolean;
  address: number;
  bytes_written: number;
  message: string;
  log_id: number | null;
  steps: InjectionStep[];
  rolled_back: boolean;
  temp_alloc_address: number | null;
  memcpy_result: number | null;
}

export interface InjectionStep {
  step: string;
  success: boolean;
  address: number | null;
  size: number | null;
  return_value: string | null;
  error: string | null;
  timestamp: string;
}

export interface InjectionLog {
  id: number;
  snapshot_id: number;
  target_pid: number;
  target_address: number;
  data_size: number;
  temp_alloc_address: number | null;
  temp_alloc_size: number | null;
  memcpy_address: number | null;
  thread_exit_code: number | null;
  success: boolean;
  rolled_back: boolean;
  steps_json: string;
  created_at: string;
}

export const categoryLabel: Record<ProcessCategory, string> = {
  Browser: "浏览器",
  IDE: "IDE",
  Design: "设计软件",
  Other: "其他",
};

export const dataTypeLabel: Record<DataType, string> = {
  Json: "JSON",
  Pickle: "Pickle/Python序列化",
  Base64: "Base64编码",
  RegexMatch: "正则匹配",
};

export const api = {
  listProcesses: () => invoke<ProcessInfo[]>("list_all_processes"),
  listProcessesByCategory: (cats: string[]) =>
    invoke<ProcessInfo[]>("list_processes_by_category", { categories: cats }),
  getRegions: (pid: number) => invoke<MemoryRegion[]>("get_process_regions", { pid }),
  readMemory: (pid: number, address: number, size: number) =>
    invoke<number[]>("read_process_memory", { pid, address, size }),
  scanMemory: (config: ScanConfig) => invoke<DataBlock[]>("scan_memory", { config }),
  createSnapshot: (params: SnapshotCreateParams) =>
    invoke<number>("create_memory_snapshot", { params }),
  autoSnapshot: (
    pid: number,
    processName: string,
    address: number,
    size: number,
    dataType: DataType,
    note?: string | null
  ) =>
    invoke<number>("auto_snapshot_at_address", {
      pid,
      processName,
      address,
      size,
      dataType,
      note,
    }),
  listSnapshots: (pid?: number | null, processName?: string | null, limit = 100, offset = 0) =>
    invoke<Snapshot[]>("list_snapshots", {
      query: { pid, process_name: processName, limit, offset },
    }),
  getSnapshot: (id: number) => invoke<Snapshot | null>("get_snapshot", { id }),
  deleteSnapshot: (id: number) => invoke<boolean>("delete_snapshot", { id }),
  updateNote: (id: number, note: string) =>
    invoke<boolean>("update_snapshot_note", { id, note }),
  countSnapshots: (pid?: number | null, processName?: string | null) =>
    invoke<number>("count_snapshots", {
      query: { pid, process_name: processName },
    }),
  injectSnapshot: (snapshotId: number, targetPid: number, targetAddress?: number | null) =>
    invoke<InjectionResult>("inject_snapshot_to_process", {
      snapshotId,
      targetPid,
      targetAddress,
    }),
  listInjectionLogs: (targetPid?: number | null, snapshotId?: number | null, limit = 100, offset = 0) =>
    invoke<InjectionLog[]>("list_injection_logs", {
      targetPid,
      snapshotId,
      limit,
      offset,
    }),
  getInjectionLog: (id: number) =>
    invoke<InjectionLog | null>("get_injection_log", { id }),
  setActivePid: (pid: number | null) => invoke<void>("set_active_pid", { pid }),
  getActivePid: () => invoke<number | null>("get_active_pid"),
  setMonitored: (pids: number[]) => invoke<void>("set_monitored_pids", { pids }),
  getMonitored: () => invoke<number[]>("get_monitored_pids"),
  showFloatingPanel: () => invoke<void>("show_floating_panel"),
  hideFloatingPanel: () => invoke<void>("hide_floating_panel"),
  listenQuickScan: (cb: () => void) =>
    event.listen("shortcut-quick-scan", () => cb()),
};
