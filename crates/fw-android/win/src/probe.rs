//! `fw-android-win probe`: what this Windows 11 machine offers a Linux guest, measured without elevation.

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::path::PathBuf;

use serde::Serialize;
use windows::Wdk::System::SystemServices::RtlGetVersion;
use windows::Win32::Foundation::{HANDLE, HLOCAL, HMODULE, LocalFree};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIDevice, IDXGIFactory1};
use windows::Win32::Security::Authorization::ConvertStringSidToSidW;
use windows::Win32::Security::{CheckTokenMembership, GetTokenInformation, PSID, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoSetProxyBlanket, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL,
    RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW};
use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::System::Variant::{VARIANT, VT_BSTR, VT_I4, VariantClear};
use windows::Win32::System::Wmi::{IWbemLocator, WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_INFINITE, WbemLocator};
use windows::core::{BOOL, BSTR, Interface, PCWSTR, w};

use crate::wsl;

const FEATURES: [&str; 4] = ["Microsoft-Hyper-V-All", "VirtualMachinePlatform", "HypervisorPlatform", "Microsoft-Windows-Subsystem-Linux"];
const HYPER_V_ADMINISTRATORS: PCWSTR = w!("S-1-5-32-578");
const GUEST_CONFIG_OPTIONS: [&str; 9] =
    ["ANDROID_BINDER_IPC", "ANDROID_BINDERFS", "MEMFD_CREATE", "DXGKRNL", "VIRTIO", "OVERLAY_FS", "EROFS_FS", "SQUASHFS", "PSI"];

#[derive(Serialize)]
pub struct Probe {
    pub windows: WindowsInfo,
    pub hypervisor: Hypervisor,
    pub features: BTreeMap<String, String>,
    pub privileges: Privileges,
    pub gpus: Vec<Gpu>,
    pub gpu_pv: GpuPv,
    pub wsl: wsl::WslInfo,
    pub guest: Option<wsl::GuestInfo>,
    pub verdict: Verdict,
}

#[derive(Serialize)]
pub struct WindowsInfo {
    pub product: String,
    pub display_version: String,
    pub version: String,
    pub build: u32,
    pub ubr: u32,
}

#[derive(Serialize)]
pub struct Hypervisor {
    pub cpuid_present: bool,
    pub cpuid_vendor: String,
    pub whp_dll: bool,
    pub whp_present: Option<bool>,
    pub whp_features: Option<String>,
}

#[derive(Serialize)]
pub struct Privileges {
    pub elevated: bool,
    pub hyper_v_administrator: bool,
}

#[derive(Serialize)]
pub struct Gpu {
    pub name: String,
    pub vendor_id: String,
    pub device_id: String,
    pub dedicated_video_memory_mib: u64,
    pub luid: String,
    pub software: bool,
    pub umd_version: Option<String>,
    pub nvidia_driver: Option<String>,
}

#[derive(Serialize)]
pub struct GpuPv {
    pub host_wsl_libs: Vec<String>,
    pub host_driver_libs: Vec<String>,
    pub nvidia_smi: Option<String>,
    pub native_hyperv_gpu_p: String,
    pub guest_dev_dxg: Option<bool>,
}

#[derive(Serialize)]
pub struct Verdict {
    pub wsl2_gpu_pv: bool,
    pub guest_kernel_has_binder: Option<bool>,
    pub native_hyperv_without_elevation: bool,
    pub missing_optional_features: Vec<String>,
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn registry_string(subkey: &str, value: &str) -> Option<String> {
    let (subkey, value) = (wide(subkey), wide(value));
    let mut size = 0u32;
    // SAFETY: buffers are sized by the first call and outlive both calls.
    unsafe {
        RegGetValueW(HKEY_LOCAL_MACHINE, PCWSTR(subkey.as_ptr()), PCWSTR(value.as_ptr()), RRF_RT_REG_SZ, None, None, Some(&mut size))
            .ok()
            .ok()?;
        let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast::<c_void>()),
            Some(&mut size),
        )
        .ok()
        .ok()?;
        let end = buffer.iter().position(|unit| *unit == 0).unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..end]))
    }
}

fn registry_dword(subkey: &str, value: &str) -> Option<u32> {
    let (subkey, value) = (wide(subkey), wide(value));
    let mut data = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: `data` is a live u32 and `size` its byte length.
    unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(std::ptr::from_mut(&mut data).cast::<c_void>()),
            Some(&mut size),
        )
        .ok()
        .ok()?;
    }
    Some(data)
}

fn windows_info() -> WindowsInfo {
    let mut info = OSVERSIONINFOW { dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32, ..Default::default() };
    // SAFETY: `info` is initialised with its size as RtlGetVersion requires.
    let _ = unsafe { RtlGetVersion(&mut info) };
    let key = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    // ProductName still says "Windows 10" on Windows 11 builds, the build number is what tells them apart.
    let product = registry_string(key, "ProductName").unwrap_or_default();
    WindowsInfo {
        product: if info.dwBuildNumber >= 22000 { product.replace("Windows 10", "Windows 11") } else { product },
        display_version: registry_string(key, "DisplayVersion").unwrap_or_default(),
        version: format!("{}.{}.{}", info.dwMajorVersion, info.dwMinorVersion, info.dwBuildNumber),
        build: info.dwBuildNumber,
        ubr: registry_dword(key, "UBR").unwrap_or_default(),
    }
}

fn cpuid_hypervisor() -> (bool, String) {
    use std::arch::x86_64::__cpuid;
    let (present, vendor) = {
        let present = __cpuid(1).ecx & (1 << 31) != 0;
        let leaf = __cpuid(0x4000_0000);
        let bytes: Vec<u8> = [leaf.ebx, leaf.ecx, leaf.edx].iter().flat_map(|register| register.to_le_bytes()).collect();
        (present, String::from_utf8_lossy(&bytes).trim_end_matches('\0').to_string())
    };
    (present, if present { vendor } else { String::new() })
}

/// WinHvPlatform.dll is loaded at run time so a machine without the Windows Hypervisor Platform still gets a report.
fn whp() -> (bool, Option<bool>, Option<String>) {
    type GetCapability = unsafe extern "system" fn(i32, *mut c_void, u32, *mut u32) -> i32;
    // SAFETY: plain library load; the exported function is called with the documented WHvGetCapability signature.
    unsafe {
        let Ok(library) = LoadLibraryW(w!("WinHvPlatform.dll")) else { return (false, None, None) };
        let library: HMODULE = library;
        let Some(symbol) = GetProcAddress(library, windows::core::s!("WHvGetCapability")) else { return (true, None, None) };
        let get_capability: GetCapability = std::mem::transmute(symbol);
        let mut present = 0u32;
        let mut written = 0u32;
        let status = get_capability(0, std::ptr::from_mut(&mut present).cast(), size_of::<u32>() as u32, &mut written);
        let present = (status >= 0).then_some(present != 0);
        let mut features = 0u64;
        let status = get_capability(1, std::ptr::from_mut(&mut features).cast(), size_of::<u64>() as u32, &mut written);
        (true, present, (status >= 0).then(|| format!("{features:#x}")))
    }
}

/// Reads a WMI property: `(string, integer)` of whichever the variant holds, then clears it.
fn variant_value(variant: &mut VARIANT) -> (Option<String>, Option<i32>) {
    // SAFETY: the union member read is the one selected by `vt`, and the variant is cleared exactly once.
    unsafe {
        let inner = &variant.Anonymous.Anonymous;
        let value = match inner.vt {
            VT_BSTR => (Some(inner.Anonymous.bstrVal.to_string()), None),
            VT_I4 => (None, Some(inner.Anonymous.lVal)),
            _ => (None, None),
        };
        let _ = VariantClear(variant);
        value
    }
}

/// `Get-WindowsOptionalFeature` needs elevation, the same data through `Win32_OptionalFeature` does not.
fn optional_features() -> BTreeMap<String, String> {
    let mut states = BTreeMap::new();
    let mut query = || -> windows::core::Result<()> {
        // SAFETY: standard WMI client sequence; every interface is released on drop.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let locator: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER)?;
            let services =
                locator.ConnectServer(&BSTR::from(r"ROOT\CIMV2"), &BSTR::new(), &BSTR::new(), &BSTR::new(), 0, &BSTR::new(), None)?;
            CoSetProxyBlanket(&services, 10, 0, None, RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE, None, EOAC_NONE)?;
            let wanted: Vec<String> = FEATURES.iter().map(|name| format!("Name = '{name}'")).collect();
            let wql = format!("SELECT Name, InstallState FROM Win32_OptionalFeature WHERE {}", wanted.join(" OR "));
            let rows =
                services.ExecQuery(&BSTR::from("WQL"), &BSTR::from(wql), WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY, None)?;
            loop {
                let mut objects = [None];
                let mut returned = 0u32;
                rows.Next(WBEM_INFINITE, &mut objects, &mut returned).ok()?;
                let Some(object) = objects[0].take() else { break };
                let (mut name, mut state) = (VARIANT::default(), VARIANT::default());
                object.Get(w!("Name"), 0, &mut name, None, None)?;
                object.Get(w!("InstallState"), 0, &mut state, None, None)?;
                let label = match variant_value(&mut state).1 {
                    Some(1) => "Enabled",
                    Some(2) => "Disabled",
                    Some(3) => "Absent",
                    _ => "Unknown",
                };
                if let (Some(name), _) = variant_value(&mut name) {
                    states.insert(name, label.to_string());
                }
            }
            Ok(())
        }
    };
    if let Err(error) = query() {
        states.insert("error".into(), error.to_string());
    }
    states
}

fn privileges() -> Privileges {
    let mut elevated = false;
    let mut member = BOOL(0);
    // SAFETY: token handle and SID are owned for the duration of the calls and released afterwards.
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_ok() {
            let mut elevation = TOKEN_ELEVATION::default();
            let mut length = 0u32;
            if GetTokenInformation(
                token,
                TokenElevation,
                Some(std::ptr::from_mut(&mut elevation).cast()),
                size_of::<TOKEN_ELEVATION>() as u32,
                &mut length,
            )
            .is_ok()
            {
                elevated = elevation.TokenIsElevated != 0;
            }
            let _ = windows::Win32::Foundation::CloseHandle(token);
        }
        let mut sid = PSID::default();
        if ConvertStringSidToSidW(HYPER_V_ADMINISTRATORS, &mut sid).is_ok() {
            let _ = CheckTokenMembership(None, sid, &mut member);
            LocalFree(Some(HLOCAL(sid.0)));
        }
    }
    Privileges { elevated, hyper_v_administrator: member.as_bool() }
}

fn adapters() -> Vec<Gpu> {
    let mut gpus = Vec::new();
    // SAFETY: DXGI enumeration; descriptors are plain data.
    unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else { return gpus };
        let mut index = 0;
        while let Ok(adapter) = factory.EnumAdapters1(index) {
            index += 1;
            let Ok(description) = adapter.GetDesc1() else {
                continue;
            };
            let end = description.Description.iter().position(|unit| *unit == 0).unwrap_or(description.Description.len());
            let umd = adapter.CheckInterfaceSupport(&IDXGIDevice::IID).ok().map(|version| {
                let parts = [(version >> 48) & 0xffff, (version >> 32) & 0xffff, (version >> 16) & 0xffff, version & 0xffff];
                (parts, format!("{}.{}.{}.{}", parts[0], parts[1], parts[2], parts[3]))
            });
            // NVIDIA's public number is the last digit of the third field followed by the fourth: 32.0.16.2114 is 621.14.
            let nvidia_driver = (description.VendorId == 0x10de).then_some(()).and_then(|()| {
                let (parts, _) = umd.as_ref()?;
                let digits = format!("{}{:04}", parts[2] % 10, parts[3]);
                let split = digits.len().checked_sub(2)?;
                Some(format!("{}.{}", &digits[..split], &digits[split..]))
            });
            gpus.push(Gpu {
                name: String::from_utf16_lossy(&description.Description[..end]),
                vendor_id: format!("{:#06x}", description.VendorId),
                device_id: format!("{:#06x}", description.DeviceId),
                dedicated_video_memory_mib: (description.DedicatedVideoMemory / (1024 * 1024)) as u64,
                luid: format!("{:08x}-{:08x}", description.AdapterLuid.HighPart, description.AdapterLuid.LowPart),
                software: description.Flags & 2 != 0,
                umd_version: umd.map(|(_, text)| text),
                nvidia_driver,
            });
        }
    }
    gpus
}

fn host_libraries() -> (Vec<String>, Vec<String>) {
    let list = |directory: Option<PathBuf>| -> Vec<String> {
        let Some(directory) = directory else { return Vec::new() };
        let mut names: Vec<String> = std::fs::read_dir(directory)
            .map(|entries| entries.filter_map(Result::ok).map(|entry| entry.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        names.sort();
        names
    };
    let program_files = std::env::var_os("ProgramFiles").map(|base| PathBuf::from(base).join("WSL").join("lib"));
    let system_root = std::env::var_os("SystemRoot").map(|base| PathBuf::from(base).join("System32").join("lxss").join("lib"));
    (list(program_files), list(system_root))
}

pub fn run(distro: &str) -> Probe {
    let features = optional_features();
    let (cpuid_present, cpuid_vendor) = cpuid_hypervisor();
    let (whp_dll, whp_present, whp_features) = whp();
    let privileges = privileges();
    let gpus = adapters();
    let (host_wsl_libs, host_driver_libs) = host_libraries();
    let wsl_info = wsl::info();
    let guest = wsl_info.distros.iter().any(|entry| entry.name == distro).then(|| wsl::guest_info(distro, &GUEST_CONFIG_OPTIONS));
    let nvidia_smi = wsl::host_nvidia_smi();
    let enabled = |name: &str| features.get(name).is_some_and(|state| state == "Enabled");
    let missing_optional_features = FEATURES.iter().filter(|name| !enabled(name)).map(|name| (*name).to_string()).collect::<Vec<_>>();
    let verdict = Verdict {
        wsl2_gpu_pv: cpuid_present
            && enabled("VirtualMachinePlatform")
            && enabled("Microsoft-Windows-Subsystem-Linux")
            && gpus.iter().any(|gpu| !gpu.software)
            && host_wsl_libs.iter().any(|name| name == "libd3d12.so")
            && guest.as_ref().is_none_or(|info| info.dev_dxg),
        guest_kernel_has_binder: guest.as_ref().map(|info| info.config.get("ANDROID_BINDER_IPC").is_some_and(|value| value == "y")),
        native_hyperv_without_elevation: privileges.elevated || privileges.hyper_v_administrator,
        missing_optional_features,
    };
    Probe {
        windows: windows_info(),
        hypervisor: Hypervisor { cpuid_present, cpuid_vendor, whp_dll, whp_present, whp_features },
        features,
        gpu_pv: GpuPv {
            host_wsl_libs,
            host_driver_libs,
            nvidia_smi,
            native_hyperv_gpu_p: if privileges.elevated || privileges.hyper_v_administrator {
                "available: run Get-VMHostPartitionableGpu".into()
            } else {
                "needs elevation or Hyper-V Administrators membership (Get-VMHostPartitionableGpu, Add-VMGpuPartitionAdapter)".into()
            },
            guest_dev_dxg: guest.as_ref().map(|info| info.dev_dxg),
        },
        privileges,
        gpus,
        wsl: wsl_info,
        guest,
        verdict,
    }
}
