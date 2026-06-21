use crate::error::{AppError, AppResult};
use crate::types::{MemoryRegion, ProcessCategory, ProcessInfo};
use std::collections::HashSet;

#[cfg(windows)]
mod platform {
    use super::*;
    use std::mem;
    use winapi::shared::minwindef::{DWORD, HMODULE, LPVOID};
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::memoryapi::{VirtualAllocEx, VirtualFreeEx, VirtualQueryEx, ReadProcessMemory, WriteProcessMemory};
    use winapi::um::processthreadsapi::{OpenProcess, CreateRemoteThread, WaitForSingleObject};
    use winapi::um::psapi::{EnumProcessModules, GetModuleBaseNameA, GetProcessMemoryInfo};
    use winapi::um::synchapi::INFINITE;
    use winapi::um::tlhelp32::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use winapi::um::winnt::{
        HANDLE, MEMORY_BASIC_INFORMATION, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE,
        PAGE_READONLY, PAGE_READWRITE, PAGE_READWRITE, MEM_COMMIT, MEM_RESERVE,
        MEM_RELEASE, PROCESS_ALL_ACCESS, PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION,
        PROCESS_VM_READ, PROCESS_VM_WRITE, PROCESS_MEMORY_COUNTERS,
    };
    use winapi::um::libloaderapi::{GetModuleHandleA, GetProcAddress};

    pub fn open_process(pid: u32, access: DWORD) -> AppResult<HANDLE> {
        unsafe {
            let handle = OpenProcess(access, 0, pid);
            if handle.is_null() {
                return Err(AppError::CannotOpenProcess {
                    pid,
                    source: format!("OpenProcess failed with error={}", last_err()).into(),
                });
            }
            Ok(handle)
        }
    }

    fn last_err() -> DWORD {
        unsafe { winapi::um::errhandlingapi::GetLastError() }
    }

    pub fn list_processes() -> AppResult<Vec<ProcessInfo>> {
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == winapi::shared::ntdef::INVALID_HANDLE_VALUE {
                return Err(AppError::Other(format!(
                    "CreateToolhelp32Snapshot failed: {}",
                    last_err()
                )));
            }

            let mut entry: PROCESSENTRY32W = mem::zeroed();
            entry.dwSize = mem::size_of::<PROCESSENTRY32W>() as DWORD;

            let mut results = Vec::new();
            let mut seen = HashSet::new();

            if Process32FirstW(snap, &mut entry) != 0 {
                loop {
                    let name = decode_wide(&entry.szExeFile);
                    if !name.is_empty() && seen.insert(entry.th32ProcessID) {
                        let category = ProcessCategory::from_name(&name);
                        let memory_mb = get_process_memory_mb(entry.th32ProcessID);
                        results.push(ProcessInfo {
                            pid: entry.th32ProcessID,
                            name: name.clone(),
                            path: get_process_path(entry.th32ProcessID, &name),
                            memory_mb,
                            category,
                        });
                    }
                    if Process32NextW(snap, &mut entry) == 0 {
                        break;
                    }
                }
            }

            CloseHandle(snap);
            Ok(results)
        }
    }

    fn decode_wide(buf: &[u16]) -> String {
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..end])
    }

    fn get_process_memory_mb(pid: u32) -> f64 {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
            if handle.is_null() {
                return 0.0;
            }
            let mut counters: PROCESS_MEMORY_COUNTERS = mem::zeroed();
            let cb = mem::size_of::<PROCESS_MEMORY_COUNTERS>() as DWORD;
            let mb = if GetProcessMemoryInfo(handle, &mut counters, cb) != 0 {
                counters.WorkingSetSize as f64 / (1024.0 * 1024.0)
            } else {
                0.0
            };
            CloseHandle(handle);
            mb
        }
    }

    fn get_process_path(pid: u32, _name: &str) -> Option<String> {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
            if handle.is_null() {
                return None;
            }
            let mut modules: [HMODULE; 1024] = mem::zeroed();
            let mut cb_needed: DWORD = 0;
            if EnumProcessModules(
                handle,
                modules.as_mut_ptr(),
                (modules.len() * mem::size_of::<HMODULE>()) as DWORD,
                &mut cb_needed,
            ) != 0
            {
                let count = (cb_needed as usize) / mem::size_of::<HMODULE>();
                if count > 0 {
                    let mut buf: [i8; 512] = mem::zeroed();
                    let len = GetModuleBaseNameA(
                        handle,
                        modules[0],
                        buf.as_mut_ptr(),
                        buf.len() as DWORD,
                    );
                    if len > 0 {
                        let s = std::ffi::CStr::from_ptr(buf.as_ptr())
                            .to_string_lossy()
                            .into_owned();
                        CloseHandle(handle);
                        return Some(s);
                    }
                }
            }
            CloseHandle(handle);
            None
        }
    }

    pub fn memory_regions(pid: u32) -> AppResult<Vec<MemoryRegion>> {
        unsafe {
            let handle = open_process(pid, PROCESS_QUERY_INFORMATION | PROCESS_VM_READ)?;
            let mut regions = Vec::new();
            let mut addr: u64 = 0;
            let max_addr: u64 = if cfg!(target_arch = "x86_64") {
                0x00007FFFFFFFFFFF
            } else {
                0x7FFFFFFF
            };

            while addr < max_addr {
                let mut mbi: MEMORY_BASIC_INFORMATION = mem::zeroed();
                let ret = VirtualQueryEx(
                    handle,
                    addr as LPVOID,
                    &mut mbi,
                    mem::size_of::<MEMORY_BASIC_INFORMATION>(),
                );
                if ret == 0 {
                    break;
                }
                let protect = mbi.Protect;
                let state = mbi.State;
                let is_readable = protect & (PAGE_READONLY | PAGE_READWRITE | PAGE_EXECUTE_READ | PAGE_EXECUTE_READWRITE) != 0
                    && state == MEM_COMMIT;
                let is_writable = protect & (PAGE_READWRITE | PAGE_EXECUTE_READWRITE) != 0;

                if is_readable {
                    regions.push(MemoryRegion {
                        base_address: mbi.BaseAddress as u64,
                        size: mbi.RegionSize as u64,
                        protection: format!("0x{:X}", protect),
                        is_readable,
                        is_writable,
                    });
                }

                let next = mbi.BaseAddress as u64 + mbi.RegionSize as u64;
                if next <= addr {
                    break;
                }
                addr = next;
            }

            CloseHandle(handle);
            Ok(regions)
        }
    }

    pub fn read_memory(pid: u32, address: u64, size: usize) -> AppResult<Vec<u8>> {
        unsafe {
            let handle = open_process(pid, PROCESS_VM_READ)?;
            let mut buf = vec![0u8; size];
            let mut bytes_read: usize = 0;

            let ret = ReadProcessMemory(
                handle,
                address as LPVOID,
                buf.as_mut_ptr() as LPVOID,
                size,
                &mut bytes_read,
            );
            CloseHandle(handle);

            if ret == 0 {
                return Err(AppError::MemoryReadFailed {
                    address,
                    source: format!("ReadProcessMemory failed, error={}", last_err()).into(),
                });
            }
            buf.truncate(bytes_read);
            Ok(buf)
        }
    }

    pub fn write_memory(pid: u32, address: u64, data: &[u8]) -> AppResult<usize> {
        unsafe {
            let handle = open_process(pid, PROCESS_VM_WRITE | PROCESS_VM_OPERATION)?;
            let mut bytes_written: usize = 0;

            let ret = WriteProcessMemory(
                handle,
                address as LPVOID,
                data.as_ptr() as LPVOID,
                data.len(),
                &mut bytes_written,
            );
            CloseHandle(handle);

            if ret == 0 {
                return Err(AppError::MemoryWriteFailed {
                    address,
                    source: format!("WriteProcessMemory failed, error={}", last_err()).into(),
                });
            }
            Ok(bytes_written)
        }
    }

    // ===== 安全注入 API =====

    pub fn virtual_alloc_ex(pid: u32, size: usize) -> AppResult<u64> {
        unsafe {
            let handle = open_process(pid, PROCESS_VM_OPERATION)?;
            let addr = VirtualAllocEx(
                handle,
                std::ptr::null_mut(),
                size,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            );
            CloseHandle(handle);
            if addr.is_null() {
                return Err(AppError::MemoryWriteFailed {
                    address: 0,
                    source: format!("VirtualAllocEx failed, size={}, error={}", size, last_err()).into(),
                });
            }
            Ok(addr as u64)
        }
    }

    pub fn virtual_free_ex(pid: u32, address: u64, size: usize) -> AppResult<()> {
        unsafe {
            let handle = open_process(pid, PROCESS_VM_OPERATION)?;
            let ret = VirtualFreeEx(
                handle,
                address as LPVOID,
                size,
                MEM_RELEASE,
            );
            CloseHandle(handle);
            if ret == 0 {
                return Err(AppError::MemoryWriteFailed {
                    address,
                    source: format!("VirtualFreeEx failed at 0x{:X}, error={}", address, last_err()).into(),
                });
            }
            Ok(())
        }
    }

    pub fn get_module_proc_address(module: &str, proc_name: &str) -> AppResult<u64> {
        unsafe {
            let module_name = std::ffi::CString::new(module)
                .map_err(|e| AppError::Other(format!("模块名无效: {}", e)))?;
            let proc_cname = std::ffi::CString::new(proc_name)
                .map_err(|e| AppError::Other(format!("函数名无效: {}", e)))?;

            let h_module = GetModuleHandleA(module_name.as_ptr() as *const i8);
            if h_module.is_null() {
                return Err(AppError::Other(format!(
                    "GetModuleHandleA({}) failed, error={}",
                    module, last_err()
                )));
            }

            let proc_addr = GetProcAddress(h_module, proc_cname.as_ptr() as *const i8);
            if proc_addr.is_none() {
                return Err(AppError::Other(format!(
                    "GetProcAddress({}!{}) failed, error={}",
                    module, proc_name, last_err()
                )));
            }

            Ok(proc_addr.unwrap() as u64)
        }
    }

    pub fn create_remote_threadAndWait(
        pid: u32,
        thread_func: u64,
        param: u64,
    ) -> AppResult<u32> {
        unsafe {
            let handle = open_process(
                pid,
                PROCESS_VM_OPERATION | PROCESS_VM_WRITE | PROCESS_VM_READ
                    | PROCESS_QUERY_INFORMATION | PROCESS_CREATE_THREAD,
            )?;

            let thread_handle = CreateRemoteThread(
                handle,
                std::ptr::null_mut(),
                0,
                Some(std::mem::transmute::<usize, unsafe extern "system" fn(*mut std::ffi::c_void) -> u32>(thread_func as usize)),
                param as LPVOID,
                0,
                std::ptr::null_mut(),
            );

            if thread_handle.is_null() {
                CloseHandle(handle);
                return Err(AppError::MemoryWriteFailed {
                    address: thread_func,
                    source: format!("CreateRemoteThread failed, error={}", last_err()).into(),
                });
            }

            let wait = WaitForSingleObject(thread_handle, INFINITE);
            if wait != 0 {
                CloseHandle(thread_handle);
                CloseHandle(handle);
                return Err(AppError::MemoryWriteFailed {
                    address: thread_func,
                    source: format!("WaitForSingleObject returned {}, error={}", wait, last_err()).into(),
                });
            }

            let mut exit_code: DWORD = 0;
            let get_exit = winapi::um::processthreadsapi::GetExitCodeThread(thread_handle, &mut exit_code);
            CloseHandle(thread_handle);
            CloseHandle(handle);

            if get_exit == 0 {
                return Err(AppError::MemoryWriteFailed {
                    address: thread_func,
                    source: format!("GetExitCodeThread failed, error={}", last_err()).into(),
                });
            }

            Ok(exit_code)
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use libproc::libproc::proc_pid;
    use mach2::kern_return;
    use mach2::port::{mach_port_name_t, mach_port_t, MACH_PORT_NULL};
    use mach2::vm::{mach_vm_read, mach_vm_write, mach_vm_region, vm_deallocate, mach_vm_allocate};
    use mach2::vm_statistics::{vm_region_basic_info_data_64_t, VM_REGION_BASIC_INFO_64, VM_REGION_BASIC_INFO_COUNT_64};
    use mach2::message::mach_msg_type_number_t;
    use mach2::vm_prot::{VM_PROT_READ, VM_PROT_WRITE, VM_PROT_ALL};
    use mach2::tasks::task_for_pid;
    use std::ptr;
    use std::mem;

    fn task_for_pid_safe(pid: u32) -> AppResult<mach_port_t> {
        unsafe {
            let mut task: mach_port_name_t = MACH_PORT_NULL;
            let kr = task_for_pid(mach2::traps::mach_task_self(), pid as i32, &mut task);
            if kr != kern_return::KERN_SUCCESS {
                return Err(AppError::CannotOpenProcess {
                    pid,
                    source: format!("task_for_pid failed: kern={}", kr).into(),
                });
            }
            Ok(task)
        }
    }

    pub fn list_processes() -> AppResult<Vec<ProcessInfo>> {
        let mut types = unsafe { std::mem::zeroed() };
        let mut count = 0;
        let pids = proc_pid::listpids(0, &mut types, &mut count)
            .map_err(|e| AppError::Other(format!("listpids: {:?}", e)))?;

        let mut results = Vec::new();
        let mut seen = HashSet::new();
        for &pid in &pids {
            let pid_u = pid as u32;
            if !seen.insert(pid_u) {
                continue;
            }
            let name = proc_pid::name(pid)
                .ok()
                .flatten()
                .unwrap_or_else(|| "unknown".to_string());
            if name.is_empty() {
                continue;
            }
            let category = ProcessCategory::from_name(&name);
            let memory_mb = get_process_memory_mb(pid_u);
            results.push(ProcessInfo {
                pid: pid_u,
                name: name.clone(),
                path: None,
                memory_mb,
                category,
            });
        }
        Ok(results)
    }

    fn get_process_memory_mb(pid: u32) -> f64 {
        use libproc::libproc::pid_rusage;
        match pid_rusage::pidrusage(pid as i32) {
            Ok(info) => {
                let bytes = info.ri_resident_size as f64;
                bytes / (1024.0 * 1024.0)
            }
            Err(_) => 0.0,
        }
    }

    pub fn memory_regions(pid: u32) -> AppResult<Vec<MemoryRegion>> {
        unsafe {
            let task = task_for_pid_safe(pid)?;
            let mut regions = Vec::new();
            let mut addr: u64 = 0;

            loop {
                let mut size: u64 = 0;
                let mut info: vm_region_basic_info_data_64_t = mem::zeroed();
                let mut info_count: mach_msg_type_number_t = VM_REGION_BASIC_INFO_COUNT_64;
                let mut object_name: mach_port_t = 0;
                let kr = mach_vm_region(
                    task,
                    &mut addr,
                    &mut size,
                    VM_REGION_BASIC_INFO_64,
                    &mut info as *mut _ as *mut i32,
                    &mut info_count,
                    &mut object_name,
                );
                if kr != kern_return::KERN_SUCCESS {
                    break;
                }

                let prot = info.protection;
                let max_prot = info.max_protection;
                let is_readable = prot & VM_PROT_READ != 0;
                let is_writable = prot & VM_PROT_WRITE != 0;

                if is_readable {
                    regions.push(MemoryRegion {
                        base_address: addr,
                        size,
                        protection: format!("prot=0x{:X} max=0x{:X}", prot, max_prot),
                        is_readable,
                        is_writable,
                    });
                }

                addr += size;
            }
            Ok(regions)
        }
    }

    pub fn read_memory(pid: u32, address: u64, size: usize) -> AppResult<Vec<u8>> {
        unsafe {
            let task = task_for_pid_safe(pid)?;
            let mut data_ptr: *mut u8 = ptr::null_mut();
            let mut data_cnt: mach_msg_type_number_t = 0;

            let kr = mach_vm_read(
                task,
                address,
                size as u64,
                &mut data_ptr as *mut *mut u8 as *mut u64,
                &mut data_cnt,
            );
            if kr != kern_return::KERN_SUCCESS {
                return Err(AppError::MemoryReadFailed {
                    address,
                    source: format!("mach_vm_read failed: kern={}", kr).into(),
                });
            }

            let slice = std::slice::from_raw_parts(data_ptr, data_cnt as usize);
            let result = slice.to_vec();
            vm_deallocate(task, data_ptr as u64, data_cnt as u64);
            Ok(result)
        }
    }

    pub fn write_memory(pid: u32, address: u64, data: &[u8]) -> AppResult<usize> {
        unsafe {
            let task = task_for_pid_safe(pid)?;
            let kr = mach_vm_write(
                task,
                address,
                data.as_ptr() as u64,
                data.len() as u32,
            );
            if kr != kern_return::KERN_SUCCESS {
                return Err(AppError::MemoryWriteFailed {
                    address,
                    source: format!("mach_vm_write failed: kern={}", kr).into(),
                });
            }
            Ok(data.len())
        }
    }

    // ===== macOS 安全注入 API =====

    pub fn virtual_alloc_ex(pid: u32, size: usize) -> AppResult<u64> {
        unsafe {
            let task = task_for_pid_safe(pid)?;
            let mut addr: u64 = 0;
            let kr = mach_vm_allocate(
                task,
                &mut addr,
                size as u64,
                1,
            );
            if kr != kern_return::KERN_SUCCESS {
                return Err(AppError::MemoryWriteFailed {
                    address: 0,
                    source: format!("mach_vm_allocate failed: kern={}", kr).into(),
                });
            }
            Ok(addr)
        }
    }

    pub fn virtual_free_ex(pid: u32, address: u64, size: usize) -> AppResult<()> {
        unsafe {
            let task = task_for_pid_safe(pid)?;
            let kr = mach_vm_allocate(
                task,
                &mut (address as u64),
                size as u64,
                0,
            );
            if kr != kern_return::KERN_SUCCESS {
                return Err(AppError::MemoryWriteFailed {
                    address,
                    source: format!("mach_vm_allocate(dealloc) failed: kern={}", kr).into(),
                });
            }
            Ok(())
        }
    }

    pub fn get_module_proc_address(_module: &str, _proc_name: &str) -> AppResult<u64> {
        Err(AppError::Other(
            "macOS 不支持 GetProcAddress 等效操作，请使用 direct mach_vm_write 注入".into(),
        ))
    }

    pub fn create_remote_thread_and_wait(
        _pid: u32,
        _thread_func: u64,
        _param: u64,
    ) -> AppResult<u32> {
        Err(AppError::Other(
            "macOS 不支持 CreateRemoteThread，已回退到直接写入模式".into(),
        ))
    }
}

pub use platform::*;
