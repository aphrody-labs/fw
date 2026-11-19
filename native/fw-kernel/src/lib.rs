use serde::{Deserialize, Serialize};

pub const KERNEL_ABI_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OsVersionInfo {
    pub major: u32,
    pub minor: u32,
    pub build: u32,
    pub platform_id: u32,
    pub release: String,
    pub raw_version: String,
    pub machine: String,
    pub nodename: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HardwareInfo {
    pub logical_cpus: u32,
    pub page_size_bytes: u64,
    pub allocation_granularity_bytes: Option<u64>,
    pub total_physical_bytes: Option<u64>,
    pub available_physical_bytes: Option<u64>,
    pub memory_load_percent: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: u32,
    pub tid: u64,
    pub handle_count: Option<u32>,
    pub working_set_bytes: Option<u64>,
    pub peak_working_set_bytes: Option<u64>,
    pub pagefile_bytes: Option<u64>,
    pub page_fault_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClockInfo {
    pub qpc_counter: Option<i64>,
    pub qpc_frequency: Option<i64>,
    pub monotonic_ns: u128,
    pub uptime_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FilesystemInfo {
    pub root_path: String,
    pub volume_name: String,
    pub filesystem_name: String,
    pub volume_serial: Option<u32>,
    pub max_component_length: Option<u32>,
    pub flags: u32,
    pub decoded_flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceProbe {
    pub kind: String,
    pub inode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LinuxKernelInfo {
    pub sysname: String,
    pub nodename: String,
    pub release: String,
    pub version: String,
    pub machine: String,
    pub proc_version: Option<String>,
    pub rust_for_linux_aligned: bool,
    pub sysfs_kernel_present: bool,
    pub cgroup_v2: bool,
    pub cgroup_path: Option<String>,
    pub namespaces: Vec<NamespaceProbe>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WindowsKernelInfo {
    pub processor_architecture: String,
    pub processor_level: u16,
    pub processor_revision: u16,
    pub active_processor_mask: u64,
    pub nt_version_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KernelSnapshot {
    pub ok: bool,
    pub crate_name: String,
    pub abi_version: u32,
    pub os_family: String,
    pub os: String,
    pub arch: String,
    pub kernel_version: OsVersionInfo,
    pub hardware: HardwareInfo,
    pub process: ProcessInfo,
    pub clock: ClockInfo,
    pub filesystem: Option<FilesystemInfo>,
    pub windows: Option<WindowsKernelInfo>,
    pub linux: Option<LinuxKernelInfo>,
}

/// Decode standard Windows `GetVolumeInformationW` filesystem feature flags.
pub fn decode_windows_fs_flags(flags: u32) -> Vec<String> {
    const KNOWN: &[(u32, &str)] = &[
        (0x0000_0001, "FILE_CASE_SENSITIVE_SEARCH"),
        (0x0000_0002, "FILE_CASE_PRESERVED_NAMES"),
        (0x0000_0004, "FILE_UNICODE_ON_DISK"),
        (0x0000_0008, "FILE_PERSISTENT_ACLS"),
        (0x0000_0010, "FILE_FILE_COMPRESSION"),
        (0x0000_0020, "FILE_VOLUME_QUOTAS"),
        (0x0000_0040, "FILE_SUPPORTS_SPARSE_FILES"),
        (0x0000_0080, "FILE_SUPPORTS_REPARSE_POINTS"),
        (0x0000_8000, "FILE_VOLUME_IS_COMPRESSED"),
        (0x0001_0000, "FILE_SUPPORTS_OBJECT_IDS"),
        (0x0002_0000, "FILE_SUPPORTS_ENCRYPTION"),
        (0x0004_0000, "FILE_NAMED_STREAMS"),
        (0x0008_0000, "FILE_READ_ONLY_VOLUME"),
        (0x0010_0000, "FILE_SEQUENTIAL_WRITE_ONCE"),
        (0x0020_0000, "FILE_SUPPORTS_TRANSACTIONS"),
        (0x0040_0000, "FILE_SUPPORTS_HARD_LINKS"),
        (0x0080_0000, "FILE_SUPPORTS_EXTENDED_ATTRIBUTES"),
        (0x0100_0000, "FILE_SUPPORTS_OPEN_BY_FILE_ID"),
        (0x0200_0000, "FILE_SUPPORTS_USN_JOURNAL"),
        (0x0400_0000, "FILE_SUPPORTS_INTEGRITY_STREAMS"),
        (0x0800_0000, "FILE_SUPPORTS_BLOCK_REFCOUNTING"),
        (0x1000_0000, "FILE_SUPPORTS_SPARSE_VDL"),
        (0x2000_0000, "FILE_DAX_VOLUME"),
        (0x4000_0000, "FILE_SUPPORTS_GHOSTING"),
    ];
    KNOWN
        .iter()
        .filter(|(bit, _)| (flags & bit) != 0)
        .map(|(_, name)| (*name).to_string())
        .collect()
}

/// Parse a kernel release string such as `"7.4.0-rc3"` or `"6.12.1-arch1"` into `(major, minor, patch)`.
pub fn parse_kernel_release_triplet(release: &str) -> (u32, u32, u32) {
    let mut parts = release.split(['.', '-', '_', '+']);
    let major = parts.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    (major, minor, patch)
}

/// Parse a Linux `/proc/self/cgroup` line (prefers unified cgroup v2 `0::/path`).
pub fn parse_proc_cgroup_path(content: &str) -> Option<String> {
    let mut fallback: Option<String> = None;
    for line in content.lines() {
        let mut fields = line.splitn(3, ':');
        let hierarchy = fields.next()?;
        let controllers = fields.next()?;
        let path = fields.next()?.trim();
        if hierarchy == "0" && controllers.is_empty() {
            return Some(path.to_string());
        }
        if fallback.is_none() && !path.is_empty() {
            fallback = Some(path.to_string());
        }
    }
    fallback
}

/// Parse a `<Key>:\s*<value> kB` entry from Linux `/proc/self/status` or `/proc/meminfo` into bytes.
pub fn parse_proc_kb_field(content: &str, key: &str) -> Option<u64> {
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix(key) {
            let trimmed = rest.trim_start_matches(':').trim();
            let kb = trimmed.split_whitespace().next()?.parse::<u64>().ok()?;
            return Some(kb.saturating_mul(1024));
        }
    }
    None
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::mem::size_of;
    use windows_sys::Wdk::System::SystemServices::RtlGetVersion;
    use windows_sys::Win32::Foundation::FALSE;
    use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW;
    use windows_sys::Win32::System::Performance::{
        QueryPerformanceCounter, QueryPerformanceFrequency,
    };
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::SystemInformation::{
        GetNativeSystemInfo, GetTickCount64, GetVersionExW, GlobalMemoryStatusEx, MEMORYSTATUSEX,
        OSVERSIONINFOW, SYSTEM_INFO,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId, GetProcessHandleCount,
    };

    fn utf16_nul_to_string(buf: &[u16]) -> String {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len])
    }

    fn processor_arch_name(code: u16) -> String {
        match code {
            0 => "INTEL".to_string(),
            5 => "ARM".to_string(),
            6 => "IA64".to_string(),
            9 => "AMD64".to_string(),
            12 => "ARM64".to_string(),
            other => format!("0x{other:04x}"),
        }
    }

    pub fn default_root_path() -> String {
        if let Ok(drive) = std::env::var("SystemDrive") {
            let trimmed = drive.trim();
            if !trimmed.is_empty() {
                return if trimmed.ends_with('\\') {
                    trimmed.to_string()
                } else {
                    format!("{trimmed}\\")
                };
            }
        }
        "C:\\".to_string()
    }

    pub fn read_qpc_ticks() -> (i64, i64) {
        let mut counter: i64 = 0;
        let mut freq: i64 = 0;
        unsafe {
            let _ = QueryPerformanceCounter(&mut counter);
            let _ = QueryPerformanceFrequency(&mut freq);
        }
        (counter, freq)
    }

    fn query_volume_info(root_path: &str) -> Option<FilesystemInfo> {
        let mut wide_root: Vec<u16> = root_path.encode_utf16().collect();
        if !wide_root.ends_with(&[b'\\' as u16]) {
            wide_root.push(b'\\' as u16);
        }
        wide_root.push(0);

        let mut vol_name = [0u16; 261];
        let mut fs_name = [0u16; 261];
        let mut serial: u32 = 0;
        let mut max_comp_len: u32 = 0;
        let mut fs_flags: u32 = 0;

        let ok = unsafe {
            GetVolumeInformationW(
                wide_root.as_ptr(),
                vol_name.as_mut_ptr(),
                vol_name.len() as u32,
                &mut serial,
                &mut max_comp_len,
                &mut fs_flags,
                fs_name.as_mut_ptr(),
                fs_name.len() as u32,
            )
        };
        if ok == FALSE {
            return None;
        }
        Some(FilesystemInfo {
            root_path: root_path.to_string(),
            volume_name: utf16_nul_to_string(&vol_name),
            filesystem_name: utf16_nul_to_string(&fs_name),
            volume_serial: Some(serial),
            max_component_length: Some(max_comp_len),
            flags: fs_flags,
            decoded_flags: decode_windows_fs_flags(fs_flags),
        })
    }

    pub fn capture_snapshot_for_root(root_path: &str) -> KernelSnapshot {
        let mut sys_info = SYSTEM_INFO::default();
        unsafe {
            GetNativeSystemInfo(&mut sys_info);
        }
        let arch_code = unsafe { sys_info.Anonymous.Anonymous.wProcessorArchitecture };
        let processor_architecture = processor_arch_name(arch_code);

        let mut osvi = OSVERSIONINFOW {
            dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
            ..Default::default()
        };
        let nt_version_source = unsafe {
            if RtlGetVersion(&mut osvi) == 0 {
                "RtlGetVersion"
            } else if GetVersionExW(&mut osvi) != FALSE {
                "GetVersionExW"
            } else {
                "unavailable"
            }
        };
        let csd = utf16_nul_to_string(&osvi.szCSDVersion);
        let release = format!(
            "{}.{}.{}",
            osvi.dwMajorVersion, osvi.dwMinorVersion, osvi.dwBuildNumber
        );
        let raw_version = if csd.is_empty() {
            format!(
                "Windows NT {}.{} (Build {})",
                osvi.dwMajorVersion, osvi.dwMinorVersion, osvi.dwBuildNumber
            )
        } else {
            format!(
                "Windows NT {}.{} (Build {}, {})",
                osvi.dwMajorVersion, osvi.dwMinorVersion, osvi.dwBuildNumber, csd
            )
        };

        let mut mem = MEMORYSTATUSEX {
            dwLength: size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        let mem_ok = unsafe { GlobalMemoryStatusEx(&mut mem) != FALSE };

        let pid = unsafe { GetCurrentProcessId() };
        let tid = unsafe { GetCurrentThreadId() } as u64;
        let proc_handle = unsafe { GetCurrentProcess() };

        let mut handle_count: u32 = 0;
        let handles_ok = unsafe { GetProcessHandleCount(proc_handle, &mut handle_count) != FALSE };

        let mut pmc = PROCESS_MEMORY_COUNTERS {
            cb: size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            ..Default::default()
        };
        let pmc_ok = unsafe {
            GetProcessMemoryInfo(
                proc_handle,
                &mut pmc,
                size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            ) != FALSE
        };

        let (qpc_counter, qpc_frequency) = read_qpc_ticks();
        let monotonic_ns = if qpc_counter > 0 && qpc_frequency > 0 {
            ((qpc_counter as u128).saturating_mul(1_000_000_000)) / (qpc_frequency as u128)
        } else {
            0
        };
        let uptime_ms = unsafe { GetTickCount64() };
        let nodename = std::env::var("COMPUTERNAME").ok();

        KernelSnapshot {
            ok: true,
            crate_name: "fw-kernel".to_string(),
            abi_version: KERNEL_ABI_VERSION,
            os_family: std::env::consts::FAMILY.to_string(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            kernel_version: OsVersionInfo {
                major: osvi.dwMajorVersion,
                minor: osvi.dwMinorVersion,
                build: osvi.dwBuildNumber,
                platform_id: osvi.dwPlatformId,
                release,
                raw_version,
                machine: processor_architecture.clone(),
                nodename,
            },
            hardware: HardwareInfo {
                logical_cpus: sys_info.dwNumberOfProcessors,
                page_size_bytes: sys_info.dwPageSize as u64,
                allocation_granularity_bytes: Some(sys_info.dwAllocationGranularity as u64),
                total_physical_bytes: mem_ok.then_some(mem.ullTotalPhys),
                available_physical_bytes: mem_ok.then_some(mem.ullAvailPhys),
                memory_load_percent: mem_ok.then_some(mem.dwMemoryLoad),
            },
            process: ProcessInfo {
                pid,
                tid,
                handle_count: handles_ok.then_some(handle_count),
                working_set_bytes: pmc_ok.then_some(pmc.WorkingSetSize as u64),
                peak_working_set_bytes: pmc_ok.then_some(pmc.PeakWorkingSetSize as u64),
                pagefile_bytes: pmc_ok.then_some(pmc.PagefileUsage as u64),
                page_fault_count: pmc_ok.then_some(pmc.PageFaultCount as u64),
            },
            clock: ClockInfo {
                qpc_counter: Some(qpc_counter),
                qpc_frequency: Some(qpc_frequency),
                monotonic_ns,
                uptime_ms: Some(uptime_ms),
            },
            filesystem: query_volume_info(root_path),
            windows: Some(WindowsKernelInfo {
                processor_architecture,
                processor_level: sys_info.wProcessorLevel,
                processor_revision: sys_info.wProcessorRevision,
                active_processor_mask: sys_info.dwActiveProcessorMask as u64,
                nt_version_source: nt_version_source.to_string(),
            }),
            linux: None,
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::ffi::CStr;
    use std::fs;
    use std::path::Path;

    pub fn default_root_path() -> String {
        "/".to_string()
    }

    pub fn read_qpc_ticks() -> (i64, i64) {
        let mut ts = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        let rc = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
        if rc == 0 {
            let ns = (ts.tv_sec as i64)
                .saturating_mul(1_000_000_000)
                .saturating_add(ts.tv_nsec as i64);
            (ns, 1_000_000_000)
        } else {
            (0, 1_000_000_000)
        }
    }

    fn c_char_slice_to_string(buf: &[libc::c_char]) -> String {
        unsafe { CStr::from_ptr(buf.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    }

    fn probe_namespaces() -> Vec<NamespaceProbe> {
        const KINDS: &[&str] = &["mnt", "pid", "net", "uts", "ipc", "user", "cgroup", "time"];
        let mut out = Vec::new();
        for &kind in KINDS {
            let path = format!("/proc/self/ns/{kind}");
            if let Ok(target) = fs::read_link(&path) {
                out.push(NamespaceProbe {
                    kind: kind.to_string(),
                    inode: target.to_string_lossy().into_owned(),
                });
            }
        }
        out
    }

    fn probe_root_filesystem(root_path: &str) -> Option<FilesystemInfo> {
        let mounts = fs::read_to_string("/proc/mounts").ok()?;
        let mut matched: Option<(String, String, String)> = None;
        for line in mounts.lines() {
            let mut parts = line.split_whitespace();
            let device = parts.next()?;
            let mountpoint = parts.next()?;
            let fstype = parts.next()?;
            if mountpoint == root_path {
                matched = Some((device.to_string(), mountpoint.to_string(), fstype.to_string()));
            }
        }
        let (device, mountpoint, fstype) = matched?;
        Some(FilesystemInfo {
            root_path: mountpoint,
            volume_name: device,
            filesystem_name: fstype,
            volume_serial: None,
            max_component_length: Some(255),
            flags: 0,
            decoded_flags: Vec::new(),
        })
    }

    pub fn capture_snapshot_for_root(root_path: &str) -> KernelSnapshot {
        let mut uts: libc::utsname = unsafe { std::mem::zeroed() };
        let uname_ok = unsafe { libc::uname(&mut uts) == 0 };
        let (sysname, nodename, release, version, machine) = if uname_ok {
            (
                c_char_slice_to_string(&uts.sysname),
                c_char_slice_to_string(&uts.nodename),
                c_char_slice_to_string(&uts.release),
                c_char_slice_to_string(&uts.version),
                c_char_slice_to_string(&uts.machine),
            )
        } else {
            (
                "Linux".to_string(),
                String::new(),
                String::new(),
                String::new(),
                std::env::consts::ARCH.to_string(),
            )
        };

        let proc_version = fs::read_to_string("/proc/version")
            .ok()
            .map(|s| s.trim().to_string());
        let (major, minor, patch) = parse_kernel_release_triplet(&release);

        let logical_cpus = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) }.max(1) as u32;
        let page_size_bytes = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(4096) as u64;

        let meminfo = fs::read_to_string("/proc/meminfo").unwrap_or_default();
        let total_physical_bytes = parse_proc_kb_field(&meminfo, "MemTotal");
        let available_physical_bytes = parse_proc_kb_field(&meminfo, "MemAvailable");
        let memory_load_percent = match (total_physical_bytes, available_physical_bytes) {
            (Some(total), Some(avail)) if total > 0 => {
                let used = total.saturating_sub(avail);
                Some(((used.saturating_mul(100)) / total) as u32)
            }
            _ => None,
        };

        let pid = unsafe { libc::getpid() } as u32;
        let tid = unsafe { libc::syscall(libc::SYS_gettid) }.max(0) as u64;
        let handle_count = fs::read_dir("/proc/self/fd")
            .ok()
            .map(|iter| iter.count() as u32);
        let proc_status = fs::read_to_string("/proc/self/status").unwrap_or_default();
        let working_set_bytes = parse_proc_kb_field(&proc_status, "VmRSS");
        let peak_working_set_bytes = parse_proc_kb_field(&proc_status, "VmHWM");
        let pagefile_bytes = parse_proc_kb_field(&proc_status, "VmSize");

        let (qpc_counter, qpc_frequency) = read_qpc_ticks();
        let monotonic_ns = qpc_counter.max(0) as u128;
        let mut boot_ts = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        let uptime_ms = if unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut boot_ts) == 0 } {
            Some(
                (boot_ts.tv_sec as u64)
                    .saturating_mul(1_000)
                    .saturating_add((boot_ts.tv_nsec as u64) / 1_000_000),
            )
        } else {
            None
        };

        let cgroup_v2 = Path::new("/sys/fs/cgroup/cgroup.controllers").exists();
        let cgroup_path = fs::read_to_string("/proc/self/cgroup")
            .ok()
            .and_then(|raw| parse_proc_cgroup_path(&raw));
        let sysfs_kernel_present = Path::new("/sys/kernel").is_dir();
        let rust_for_linux_aligned = major > 6 || (major == 6 && minor >= 1);

        KernelSnapshot {
            ok: true,
            crate_name: "fw-kernel".to_string(),
            abi_version: KERNEL_ABI_VERSION,
            os_family: std::env::consts::FAMILY.to_string(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            kernel_version: OsVersionInfo {
                major,
                minor,
                build: patch,
                platform_id: 0,
                release: release.clone(),
                raw_version: proc_version.clone().unwrap_or_else(|| version.clone()),
                machine: machine.clone(),
                nodename: (!nodename.is_empty()).then_some(nodename.clone()),
            },
            hardware: HardwareInfo {
                logical_cpus,
                page_size_bytes,
                allocation_granularity_bytes: Some(page_size_bytes),
                total_physical_bytes,
                available_physical_bytes,
                memory_load_percent,
            },
            process: ProcessInfo {
                pid,
                tid,
                handle_count,
                working_set_bytes,
                peak_working_set_bytes,
                pagefile_bytes,
                page_fault_count: None,
            },
            clock: ClockInfo {
                qpc_counter: Some(qpc_counter),
                qpc_frequency: Some(qpc_frequency),
                monotonic_ns,
                uptime_ms,
            },
            filesystem: probe_root_filesystem(root_path),
            windows: None,
            linux: Some(LinuxKernelInfo {
                sysname,
                nodename,
                release,
                version,
                machine,
                proc_version,
                rust_for_linux_aligned,
                sysfs_kernel_present,
                cgroup_v2,
                cgroup_path,
                namespaces: probe_namespaces(),
            }),
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    pub fn default_root_path() -> String {
        "/".to_string()
    }

    pub fn read_qpc_ticks() -> (i64, i64) {
        let ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);
        (ns, 1_000_000_000)
    }

    pub fn capture_snapshot_for_root(_root_path: &str) -> KernelSnapshot {
        let (qpc_counter, qpc_frequency) = read_qpc_ticks();
        let cpus = std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(1);
        KernelSnapshot {
            ok: true,
            crate_name: "fw-kernel".to_string(),
            abi_version: KERNEL_ABI_VERSION,
            os_family: std::env::consts::FAMILY.to_string(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            kernel_version: OsVersionInfo {
                major: 0,
                minor: 0,
                build: 0,
                platform_id: 0,
                release: std::env::consts::OS.to_string(),
                raw_version: std::env::consts::OS.to_string(),
                machine: std::env::consts::ARCH.to_string(),
                nodename: None,
            },
            hardware: HardwareInfo {
                logical_cpus: cpus,
                page_size_bytes: 4096,
                allocation_granularity_bytes: None,
                total_physical_bytes: None,
                available_physical_bytes: None,
                memory_load_percent: None,
            },
            process: ProcessInfo {
                pid: std::process::id(),
                tid: 0,
                handle_count: None,
                working_set_bytes: None,
                peak_working_set_bytes: None,
                pagefile_bytes: None,
                page_fault_count: None,
            },
            clock: ClockInfo {
                qpc_counter: Some(qpc_counter),
                qpc_frequency: Some(qpc_frequency),
                monotonic_ns: qpc_counter.max(0) as u128,
                uptime_ms: None,
            },
            filesystem: None,
            windows: None,
            linux: None,
        }
    }
}

/// Capture a live cross-OS kernel and process snapshot for the default system volume/root.
pub fn capture_snapshot() -> KernelSnapshot {
    let root = platform::default_root_path();
    platform::capture_snapshot_for_root(&root)
}

/// Capture a live cross-OS kernel snapshot targeting a specific root path (e.g. `"C:\\"` or `"/"`).
pub fn capture_snapshot_for_root(root_path: &str) -> KernelSnapshot {
    platform::capture_snapshot_for_root(root_path)
}

/// Capture a live kernel snapshot serialized to compact JSON.
pub fn capture_snapshot_json() -> String {
    let snap = capture_snapshot();
    serde_json::to_string(&snap)
        .unwrap_or_else(|e| serde_json::json!({ "ok": false, "error": e.to_string() }).to_string())
}

/// Read the OS high-resolution monotonic tick counter and its frequency in Hz.
pub fn read_qpc_ticks() -> (i64, i64) {
    platform::read_qpc_ticks()
}

fn write_json_out(json: &str, ob: *mut u8, oc: usize) -> usize {
    if ob.is_null() || json.is_empty() || json.len() > oc {
        return 0;
    }
    let bytes = json.as_bytes();
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ob, bytes.len());
    }
    bytes.len()
}

#[unsafe(no_mangle)]
pub extern "C" fn fw_kernel_abi_version() -> u32 {
    KERNEL_ABI_VERSION
}

#[unsafe(no_mangle)]
pub extern "C" fn fw_kernel_qpc_now() -> i64 {
    read_qpc_ticks().0
}

#[unsafe(no_mangle)]
pub extern "C" fn fw_kernel_snapshot_json(ob: *mut u8, oc: usize) -> usize {
    if ob.is_null() || oc == 0 {
        return 0;
    }
    let json = capture_snapshot_json();
    write_json_out(&json, ob, oc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_captures_live_os_and_hardware() {
        let snap = capture_snapshot();
        assert!(snap.ok);
        assert_eq!(snap.crate_name, "fw-kernel");
        assert_eq!(snap.abi_version, KERNEL_ABI_VERSION);
        assert_eq!(snap.os, std::env::consts::OS);
        assert_eq!(snap.arch, std::env::consts::ARCH);
        assert!(snap.hardware.logical_cpus >= 1);
        assert!(snap.hardware.page_size_bytes >= 4096);
        assert_eq!(snap.hardware.page_size_bytes % 4096, 0);
        assert_eq!(snap.process.pid, std::process::id());
        assert!(snap.process.tid > 0);
        assert!(snap.clock.monotonic_ns > 0);

        let json = serde_json::to_string(&snap).expect("serialize snapshot");
        let decoded: KernelSnapshot = serde_json::from_str(&json).expect("deserialize snapshot");
        assert_eq!(decoded, snap);
    }

    #[test]
    fn qpc_counter_is_monotonic_and_positive() {
        let (t1, f1) = read_qpc_ticks();
        std::thread::yield_now();
        let (t2, f2) = read_qpc_ticks();
        assert!(f1 > 0);
        assert_eq!(f1, f2);
        assert!(t1 > 0);
        assert!(t2 >= t1);
        assert!(fw_kernel_qpc_now() >= t2);
    }

    #[test]
    fn ffi_fw_kernel_snapshot_json_roundtrip_and_guards() {
        assert_eq!(fw_kernel_abi_version(), KERNEL_ABI_VERSION);
        assert_eq!(fw_kernel_snapshot_json(std::ptr::null_mut(), 1024), 0);

        let mut tiny = [0u8; 8];
        assert_eq!(
            fw_kernel_snapshot_json(tiny.as_mut_ptr(), tiny.len()),
            0
        );
        assert_eq!(fw_kernel_snapshot_json(tiny.as_mut_ptr(), 0), 0);

        let mut buf = vec![0u8; 16 * 1024];
        let written = fw_kernel_snapshot_json(buf.as_mut_ptr(), buf.len());
        assert!(
            written > 64,
            "expected non-trivial JSON payload, got {written}"
        );
        let json = std::str::from_utf8(&buf[..written]).expect("valid utf8");
        let parsed: KernelSnapshot = serde_json::from_str(json).expect("valid KernelSnapshot JSON");
        assert!(parsed.ok);
        assert_eq!(parsed.process.pid, std::process::id());
        assert!(parsed.hardware.logical_cpus >= 1);
    }

    #[cfg(windows)]
    #[test]
    fn windows_ntfs_volume_and_process_telemetry() {
        let snap = capture_snapshot();
        assert_eq!(snap.os, "windows");
        assert!(snap.windows.is_some());
        assert!(snap.linux.is_none());

        assert!(snap.kernel_version.major >= 10);
        assert!(snap.kernel_version.build >= 10240);
        assert!(snap.hardware.total_physical_bytes.unwrap_or(0) > 0);
        assert!(snap.hardware.allocation_granularity_bytes.unwrap_or(0) >= 4096);
        assert!(snap.process.handle_count.unwrap_or(0) > 0);
        assert!(snap.process.working_set_bytes.unwrap_or(0) > 0);
        assert!(snap.clock.uptime_ms.unwrap_or(0) > 0);

        let fs = snap.filesystem.expect("expected Windows volume info");
        assert!(!fs.filesystem_name.is_empty());
        assert!(fs.flags != 0);
        assert!(!fs.decoded_flags.is_empty());
        assert!(fs.max_component_length.unwrap_or(0) >= 255);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_procfs_and_rfl_probes() {
        let snap = capture_snapshot();
        assert_eq!(snap.os, "linux");
        assert!(snap.linux.is_some());
        assert!(snap.windows.is_none());

        let linux = snap.linux.unwrap();
        assert_eq!(linux.sysname, "Linux");
        assert!(!linux.release.is_empty());
        assert!(linux.proc_version.is_some());
        assert!(!linux.namespaces.is_empty());
    }

    #[test]
    fn helpers_decode_ntfs_flags_and_linux_parsers() {
        let decoded = decode_windows_fs_flags(0x0000_0002 | 0x0000_0004 | 0x0000_0008 | 0x0200_0000);
        assert_eq!(
            decoded,
            vec![
                "FILE_CASE_PRESERVED_NAMES",
                "FILE_UNICODE_ON_DISK",
                "FILE_PERSISTENT_ACLS",
                "FILE_SUPPORTS_USN_JOURNAL",
            ]
        );

        assert_eq!(parse_kernel_release_triplet("7.4.0-rc3"), (7, 4, 0));
        assert_eq!(parse_kernel_release_triplet("6.12.5-arch1-1"), (6, 12, 5));

        let cgroup_sample = "12:memory:/system.slice\n0::/user.slice/user-1000.slice/session-1.scope\n";
        assert_eq!(
            parse_proc_cgroup_path(cgroup_sample).as_deref(),
            Some("/user.slice/user-1000.slice/session-1.scope")
        );

        let status_sample = "Name:\tfw\nVmRSS:\t   12340 kB\nVmHWM:\t   15000 kB\n";
        assert_eq!(
            parse_proc_kb_field(status_sample, "VmRSS"),
            Some(12340 * 1024)
        );
        assert_eq!(
            parse_proc_kb_field(status_sample, "VmHWM"),
            Some(15000 * 1024)
        );
    }
}
