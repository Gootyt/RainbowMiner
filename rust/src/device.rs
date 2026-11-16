//! Device detection and selection (Modules/DeviceLib.psm1):
//! GPUs via OpenCL (fallback nvidia-smi / Linux sysfs / Windows CIM), CPU via CPUID, /proc and /sys,
//! device naming (GPU#00, CPU#00), the devices.json selectors, device combos and the live sensor data.
#![allow(dead_code)]
#![allow(unused_unsafe)]

#![allow(non_snake_case)]

use crate::jsonx;
use crate::opencl;
use crate::util;
use crate::log_info;
use indexmap::IndexMap;
use regex::Regex;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{LazyLock, Mutex};

#[derive(Debug, Clone, Serialize, Default)]
pub struct OpenClInfo {
    pub DeviceIndex: usize,
    pub Name: String,
    pub Architecture: String,
    pub Type: String,
    pub Vendor: String,
    pub GlobalMemSize: u64,
    pub GlobalMemSizeGB: i64,
    pub MaxMemAllocSize: u64,
    pub MaxComputeUnits: u32,
    pub PlatformVersion: String,
    pub DriverVersion: String,
    pub PCIBusId: Option<String>,
    pub DeviceCapability: Option<String>,
    pub CardId: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DeviceData {
    pub AdapterId: i64,
    pub Utilization: f64,
    pub UtilizationMem: f64,
    pub Clock: f64,
    pub ClockMem: f64,
    pub FanSpeed: f64,
    pub Temperature: f64,
    pub PowerDraw: f64,
    pub PowerDrawEstimated: bool,
    pub PowerLimit: f64,
    pub PowerLimitPercent: f64,
    pub PowerMaxLimit: f64,
    pub PowerDefaultLimit: f64,
    pub Pstate: String,
    pub Method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub Cores: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub Threads: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub CacheL3: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DeviceDataMax {
    pub Clock: f64,
    pub ClockMem: f64,
    pub Temperature: f64,
    pub FanSpeed: f64,
    pub PowerDraw: f64,
    pub Utilization: f64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Device {
    pub Name: String,
    pub Index: usize,
    pub PlatformId: usize,
    pub PlatformId_Index: usize,
    pub Type_PlatformId_Index: usize,
    pub Platform_Vendor: String,
    pub Vendor: String,
    pub Vendor_Name: String,
    pub Vendor_Index: usize,
    /// "Gpu" or "Cpu"
    pub Type: String,
    pub Type_Index: usize,
    pub Type_Codec_Index: usize,
    pub Type_Mineable_Index: usize,
    pub Type_Vendor_Index: usize,
    pub BusId_Index: usize,
    pub BusId_Type_Index: usize,
    pub BusId_Type_Codec_Index: usize,
    pub BusId_Type_Vendor_Index: usize,
    pub BusId_Type_Mineable_Index: usize,
    pub BusId_Vendor_Index: usize,
    pub OpenCL: OpenClInfo,
    pub Codec: String,
    pub Model: String,
    pub Model_Base: String,
    pub Model_Name: String,
    pub InstanceId: String,
    pub CardId: i64,
    pub BusId: Option<String>,
    pub SubId: String,
    pub IsLHR: bool,
    pub ReservedVRAMGB: Option<f64>,
    pub GpuGroup: String,
    pub Features: Vec<String>,
    pub Data: DeviceData,
    pub DataMax: DeviceDataMax,
}

impl Device {
    pub fn is_gpu(&self) -> bool {
        self.Type.eq_ignore_ascii_case("Gpu")
    }
    pub fn is_cpu(&self) -> bool {
        self.Type.eq_ignore_ascii_case("Cpu")
    }
    /// vendor type key of DevicesByTypes: AMD/INTEL/NVIDIA for GPUs, CPU for CPUs
    pub fn type_key(&self) -> &str {
        if self.is_cpu() {
            "CPU"
        } else {
            &self.Vendor
        }
    }
    /// property by name (for the devices.json selectors and custom miners)
    pub fn prop(&self, name: &str) -> String {
        match name.to_lowercase().as_str() {
            "index" => self.Index.to_string(),
            "platformid" => self.PlatformId.to_string(),
            "platformid_index" => self.PlatformId_Index.to_string(),
            "type_platformid_index" => self.Type_PlatformId_Index.to_string(),
            "vendor" => self.Vendor.clone(),
            "vendor_index" => self.Vendor_Index.to_string(),
            "type" => self.Type.clone(),
            "type_index" => self.Type_Index.to_string(),
            "type_codec_index" => self.Type_Codec_Index.to_string(),
            "type_mineable_index" => self.Type_Mineable_Index.to_string(),
            "type_vendor_index" => self.Type_Vendor_Index.to_string(),
            "busid_index" => self.BusId_Index.to_string(),
            "busid_type_index" => self.BusId_Type_Index.to_string(),
            "busid_type_codec_index" => self.BusId_Type_Codec_Index.to_string(),
            "busid_type_vendor_index" => self.BusId_Type_Vendor_Index.to_string(),
            "busid_type_mineable_index" => self.BusId_Type_Mineable_Index.to_string(),
            "busid_vendor_index" => self.BusId_Vendor_Index.to_string(),
            "cardid" => self.CardId.to_string(),
            "instanceid" => self.InstanceId.clone(),
            "subid" => self.SubId.clone(),
            "model_base" => self.Model_Base.clone(),
            "busid" => self.BusId.clone().unwrap_or_default(),
            "pcibusid" => self.OpenCL.PCIBusId.clone().unwrap_or_default(),
            "model" => self.Model.clone(),
            "model_name" => self.Model_Name.clone(),
            "name" => self.Name.clone(),
            _ => String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct CpuInfo {
    pub Name: String,
    pub Manufacturer: String,
    pub Vendor: String,
    pub Cores: usize,
    pub Threads: usize,
    pub PhysicalCPUs: usize,
    pub L3CacheSize: i64,
    pub MaxClockSpeed: i64,
    pub TDP: f64,
    pub Family: i64,
    pub Model: i64,
    pub Stepping: i64,
    pub Architecture: String,
    pub Features: BTreeMap<String, bool>,
    pub RealCores: Vec<usize>,
    pub ThreadList: Vec<usize>,
    pub IsRyzen: bool,
}

impl CpuInfo {
    pub fn has(&self, f: &str) -> bool {
        self.Features.get(&f.to_lowercase()).copied().unwrap_or(false)
    }
    pub fn is_arm(&self) -> bool {
        self.Vendor == "ARM" || self.has("arm")
    }
    pub fn facts(&self) -> crate::config::CpuFacts {
        crate::config::CpuFacts {
            cores: self.Cores,
            threads: self.Threads,
            real_cores: self.RealCores.clone(),
            thread_list: self.ThreadList.clone(),
        }
    }
}

// ----------------------------------------------------------------------------------------
// helpers
// ----------------------------------------------------------------------------------------

fn read_json(base: &Path, name: &str) -> Value {
    util::read_text(&base.join("Data").join(name))
        .ok()
        .and_then(|t| jsonx::parse(&t))
        .unwrap_or(Value::Null)
}

/// run a command, return stdout (None on failure / non zero exit code)
pub fn run_cmd(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Get-NormalizedDeviceName
pub fn normalize_device_name(name: &str, vendor: &str) -> String {
    static RE_PAREN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\([A-Z0-9 ]+\)?").unwrap());
    static RE_WS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
    let mut n = RE_PAREN.replace_all(name, "").to_string();
    if vendor == "AMD" {
        for w in ["ASUS", "AMD", "Series", "Graphics", "Adapter"] {
            n = Regex::new(&format!("(?i){}", w)).unwrap().replace_all(&n, "").to_string();
        }
        n = Regex::new(r"(?i)\d+GB$").unwrap().replace_all(n.trim(), "").to_string();
        n = RE_WS.replace_all(&n, " ").trim().to_string();
        let padded = format!(" {}", n);
        if let Some(c) = Regex::new(r"(?i).*\s(HD)\s?(\w+).*").unwrap().captures(&padded) {
            return format!("Radeon HD {}", &c[2]);
        }
        if let Some(c) = Regex::new(r"(?i).*\s(Vega).*(56|64).*").unwrap().captures(&padded) {
            return format!("Radeon Vega {}", &c[2]);
        }
        if let Some(c) = Regex::new(r"(?i).*\s(R\d)\s(\w+).*").unwrap().captures(&padded) {
            return format!("Radeon {} {}", &c[1], &c[2]);
        }
        if let Some(c) = Regex::new(r"(?i).*Radeon.*(5[567]00[\w\s]*)").unwrap().captures(&padded) {
            return format!("Radeon RX {}", c[1].trim());
        }
        if let Some(c) = Regex::new(r"(?i).*Radeon.*([4-5]\d0).*").unwrap().captures(&padded) {
            return format!("Radeon RX {}", &c[1]);
        }
        n
    } else if !vendor.is_empty() {
        let n = Regex::new(&format!("(?i){}", regex::escape(vendor))).unwrap().replace_all(&n, "").to_string();
        RE_WS.replace_all(&n, " ").trim().to_string()
    } else {
        RE_WS.replace_all(&n, " ").trim().to_string()
    }
}

/// Model from a device name: "GeForce RTX 3080" -> "RTX3080"
pub fn model_from_name(name: &str) -> String {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)GeForce|Radeon|Intel").unwrap());
    let alnum: String = name.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    RE.replace_all(&alnum, "").to_string()
}

fn nvidia_architecture(base: &Path, model: &str, capability: &str) -> String {
    let db = read_json(base, "nvidiaarchdb.json");
    let cap: String = capability.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    if let Value::Object(m) = &db {
        if !cap.is_empty() {
            for (k, v) in m {
                if jsonx::str_list(jsonx::get(v, "Compute")).contains(&cap) {
                    return k.clone();
                }
            }
        }
        for (k, v) in m {
            let pat = jsonx::str_list(jsonx::get(v, "Model")).join("|");
            if !pat.is_empty() {
                if let Ok(re) = Regex::new(&format!("(?i){}", pat)) {
                    if re.is_match(model) {
                        return k.clone();
                    }
                }
            }
        }
    }
    "Other".into()
}

fn amd_compute_capability(base: &Path, model: &str, architecture: &str) -> String {
    let arch = match Regex::new(r"^(gfx\d+)").unwrap().captures(architecture) {
        Some(c) => c[1].to_string(),
        None => architecture.split(':').next().unwrap_or("").chars().filter(|c| c.is_ascii_alphanumeric()).collect(),
    };
    let db = read_json(base, "amdarchdb.json");
    if let Value::Object(m) = &db {
        for (k, v) in m {
            let pat = jsonx::str_list(v).join("|");
            if pat.is_empty() {
                continue;
            }
            if let Ok(re) = Regex::new(&format!("(?i){}", pat)) {
                if re.is_match(model) || re.is_match(&arch) {
                    return k.clone();
                }
            }
        }
    }
    arch
}

/// nvidia-smi executable (Get-NvidiaSmi)
pub fn nvidia_smi_path(base: &Path, nvsmi_path: &str) -> Option<String> {
    if cfg!(windows) {
        let p = Path::new(nvsmi_path).join("nvidia-smi.exe");
        if p.exists() {
            return Some(p.display().to_string());
        }
        let p = base.join("Includes").join("nvidia-smi.exe");
        if p.exists() {
            return Some(p.display().to_string());
        }
        let sys = PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into())).join("System32").join("nvidia-smi.exe");
        if sys.exists() {
            return Some(sys.display().to_string());
        }
        None
    } else {
        Command::new("nvidia-smi").arg("-h").output().ok().map(|_| "nvidia-smi".to_string())
    }
}

/// Invoke-NvidiaSmi: query fields, returns one map per GPU (field names with '.' -> '_')
pub fn invoke_nvidia_smi(smi: &str, fields: &[&str]) -> Vec<HashMap<String, Value>> {
    let q = format!("--query-gpu={}", fields.join(","));
    let Some(out) = run_cmd(smi, &[&q, "--format=csv,noheader,nounits"]) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for line in out.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split(',').map(|c| c.trim()).collect();
        if cols.len() < fields.len() {
            continue;
        }
        let mut m = HashMap::new();
        for (f, c) in fields.iter().zip(cols) {
            let key = f.replace('.', "_");
            let v = if c.starts_with('[') || c.is_empty() || c.eq_ignore_ascii_case("N/A") {
                Value::Null
            } else if let Ok(x) = c.parse::<f64>() {
                json!(x)
            } else {
                json!(c)
            };
            m.insert(key, v);
        }
        rows.push(m);
    }
    rows
}

fn cuda_version(smi: &str) -> Option<String> {
    let out = run_cmd(smi, &[])?;
    let re = Regex::new(r"CUDA.+?:\s*(\d+\.\d+)").unwrap();
    re.captures(&out).map(|c| format!("{}.0", &c[1]))
}

// ----------------------------------------------------------------------------------------
// GPU fallbacks without OpenCL
// ----------------------------------------------------------------------------------------

fn nvidia_smi_platform(smi: &str) -> Option<opencl::ClPlatform> {
    let cuda = cuda_version(smi).unwrap_or_default();
    let mut rows = invoke_nvidia_smi(smi, &["index", "gpu_name", "memory.total", "pci.bus_id", "pci.device_id", "driver_version", "compute_cap"]);
    if rows.is_empty() {
        rows = invoke_nvidia_smi(smi, &["index", "gpu_name", "memory.total", "pci.bus_id", "pci.device_id", "driver_version"]);
    }
    if rows.is_empty() {
        return None;
    }
    rows.sort_by_key(|r| jsonx::num(r.get("index").unwrap_or(&Value::Null)) as i64);
    let re_bus = Regex::new(r"(?i):([0-9A-F]{2}:[0-9A-F]{2})").unwrap();
    let devices = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let g = |k: &str| r.get(k).cloned().unwrap_or(Value::Null);
            let mem_mb = jsonx::num(&g("memory_total"));
            let bus = jsonx::s(&g("pci_bus_id"));
            opencl::ClDevice {
                device_index: i,
                name: jsonx::s(&g("gpu_name")),
                architecture: jsonx::s(&g("gpu_name")),
                dtype: "Gpu".into(),
                vendor: "NVIDIA Corporation".into(),
                global_mem_size: (mem_mb * 1048576.0) as u64,
                max_mem_alloc_size: 0,
                max_compute_units: 0,
                platform_version: format!("CUDA {}", cuda),
                driver_version: jsonx::s(&g("driver_version")),
                pci_bus_id: re_bus.captures(&bus).map(|c| c[1].to_uppercase()),
                device_capability: {
                    let c = jsonx::s(&g("compute_cap"));
                    if c.is_empty() {
                        None
                    } else {
                        Some(c)
                    }
                },
            }
        })
        .collect();
    Some(opencl::ClPlatform {
        platform_id: 0,
        vendor: "NVIDIA".into(),
        name: "NVIDIA nvidia-smi".into(),
        devices,
    })
}

#[derive(Debug, Clone, Default)]
struct SysfsGpu {
    vendor: String,
    bus: String,
    device_id: String,
    name: String,
    vram: u64,
    card: i64,
}

#[cfg(target_os = "linux")]
fn sysfs_gpus(base: &Path) -> Vec<SysfsGpu> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir("/sys/class/drm") else { return out };
    let amd_cards = read_json(base, "amd-cards.json");
    let intel_cards = read_json(base, "intel-cards.json");
    let re_card = Regex::new(r"^card(\d+)$").unwrap();
    for e in rd.flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        let Some(c) = re_card.captures(&n) else { continue };
        let card: i64 = c[1].parse().unwrap_or(-1);
        let dev = e.path().join("device");
        let rd = |f: &str| std::fs::read_to_string(dev.join(f)).map(|s| s.trim().to_string()).unwrap_or_default();
        let vendor_id = rd("vendor").to_lowercase();
        let class = rd("class").to_lowercase();
        if !(class.starts_with("0x0300") || class.starts_with("0x0380")) {
            continue;
        }
        let vendor = match vendor_id.as_str() {
            "0x1002" => "AMD",
            "0x8086" => "INTEL",
            "0x10de" => "NVIDIA",
            _ => continue,
        };
        let device_id = rd("device").trim_start_matches("0x").to_uppercase();
        let bus = std::fs::canonicalize(&dev)
            .ok()
            .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
            .and_then(|f| {
                // 0000:03:00.0 -> 03:00
                let parts: Vec<&str> = f.split(':').collect();
                if parts.len() == 3 {
                    Some(format!("{}:{}", parts[1], parts[2].split('.').next().unwrap_or("00")).to_uppercase())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let mut name = rd("product_name");
        if name.is_empty() {
            let db = if vendor == "AMD" { &amd_cards } else { &intel_cards };
            if vendor == "INTEL" {
                name = jsonx::s(jsonx::get(db, &device_id.to_lowercase()));
            } else {
                for series in jsonx::values(db) {
                    let n = jsonx::s(jsonx::get(series, &device_id));
                    if !n.is_empty() {
                        name = n;
                        break;
                    }
                }
            }
        }
        if name.is_empty() {
            name = if vendor == "AMD" { format!("Radeon {}", device_id) } else { format!("{} Graphics [0x{}]", vendor, device_id.to_lowercase()) };
        }
        let vram = rd("mem_info_vram_total").parse::<u64>().unwrap_or(0);
        out.push(SysfsGpu {
            vendor: vendor.into(),
            bus,
            device_id,
            name,
            vram,
            card,
        });
    }
    out.sort_by(|a, b| a.bus.cmp(&b.bus));
    out
}

#[cfg(not(target_os = "linux"))]
fn sysfs_gpus(_base: &Path) -> Vec<SysfsGpu> {
    Vec::new()
}

#[cfg(windows)]
fn windows_gpus() -> Vec<SysfsGpu> {
    // CIM via powershell: Name, AdapterCompatibility, PNPDeviceID, the memory size from the registry
    let script = r#"$ErrorActionPreference='SilentlyContinue'; $r=@(); Get-CimInstance Win32_VideoController | ForEach-Object { $m=0; Get-ItemProperty 'HKLM:\SYSTEM\ControlSet001\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0*' | Where-Object {$_.MatchingDeviceId -and $_.'HardwareInformation.qwMemorySize' -and ($_.PSPath -ne $null)} | ForEach-Object { if ("$($_.DriverDesc)" -eq "$($_.DriverDesc)" -and $_.DriverDesc -eq $_.DriverDesc) {} }; $r += [PSCustomObject]@{Name=$_.Name;Vendor=$_.AdapterCompatibility;PNP=$_.PNPDeviceID;RAM=$_.AdapterRAM} }; $r | ConvertTo-Json -Compress"#;
    let Some(out) = run_cmd("powershell", &["-NoProfile", "-NonInteractive", "-Command", script]) else {
        return Vec::new();
    };
    let v = jsonx::parse(&out).unwrap_or(Value::Null);
    let mut res = Vec::new();
    for x in jsonx::as_list(&v) {
        let vendor_raw = jsonx::s(jsonx::get(x, "Vendor"));
        let vendor = if vendor_raw.contains("Advanced Micro") || vendor_raw.contains("AMD") {
            "AMD"
        } else if vendor_raw.contains("Intel") {
            "INTEL"
        } else if vendor_raw.contains("NVIDIA") {
            "NVIDIA"
        } else {
            continue;
        };
        let pnp = jsonx::s(jsonx::get(x, "PNP"));
        let dev = Regex::new(r"DEV_([0-9A-F]{4})").unwrap().captures(&pnp).map(|c| c[1].to_string()).unwrap_or_default();
        res.push(SysfsGpu {
            vendor: vendor.into(),
            bus: String::new(),
            device_id: dev,
            name: jsonx::s(jsonx::get(x, "Name")),
            vram: jsonx::num(jsonx::get(x, "RAM")) as u64,
            card: -1,
        });
    }
    res
}

#[cfg(not(windows))]
fn windows_gpus() -> Vec<SysfsGpu> {
    Vec::new()
}

fn sysfs_platform(gpus: &[SysfsGpu], vendor: &str) -> Option<opencl::ClPlatform> {
    let devs: Vec<opencl::ClDevice> = gpus
        .iter()
        .filter(|g| g.vendor == vendor)
        .enumerate()
        .map(|(i, g)| opencl::ClDevice {
            device_index: i,
            name: g.name.clone(),
            architecture: String::new(),
            dtype: "Gpu".into(),
            vendor: match vendor {
                "AMD" => "Advanced Micro Devices, Inc.".into(),
                "INTEL" => "Intel(R) Corporation".into(),
                _ => "NVIDIA Corporation".into(),
            },
            global_mem_size: g.vram,
            max_mem_alloc_size: 0,
            max_compute_units: 0,
            platform_version: "sysfs".into(),
            driver_version: String::new(),
            pci_bus_id: if g.bus.is_empty() { None } else { Some(g.bus.clone()) },
            device_capability: None,
        })
        .collect();
    if devs.is_empty() {
        None
    } else {
        Some(opencl::ClPlatform {
            platform_id: 0,
            vendor: vendor.into(),
            name: format!("{} (no OpenCL)", vendor),
            devices: devs,
        })
    }
}

// ----------------------------------------------------------------------------------------
// CPU
// ----------------------------------------------------------------------------------------

#[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
fn cpuid_features(features: &mut BTreeMap<String, bool>) -> (i64, i64, i64, String) {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::__cpuid;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::__cpuid;
    let r0 = unsafe { __cpuid(0) };
    let mut vendor = Vec::new();
    vendor.extend_from_slice(&r0.ebx.to_le_bytes());
    vendor.extend_from_slice(&r0.edx.to_le_bytes());
    vendor.extend_from_slice(&r0.ecx.to_le_bytes());
    let vendor = String::from_utf8_lossy(&vendor).to_string();
    let n_ids = r0.eax;
    let n_ex = unsafe { __cpuid(0x8000_0000) }.eax;
    let (mut family, mut model, mut stepping) = (0i64, 0i64, 0i64);
    let mut set = |k: &str, b: bool| {
        if b {
            features.insert(k.to_string(), true);
        }
    };
    if n_ids >= 1 {
        let r = unsafe { __cpuid(1) };
        set("mmx", r.edx & (1 << 23) != 0);
        set("sse", r.edx & (1 << 25) != 0);
        set("sse2", r.edx & (1 << 26) != 0);
        set("sse3", r.ecx & 1 != 0);
        set("ssse3", r.ecx & (1 << 9) != 0);
        set("sse41", r.ecx & (1 << 19) != 0);
        set("sse42", r.ecx & (1 << 20) != 0);
        set("aes", r.ecx & (1 << 25) != 0);
        set("avx", r.ecx & (1 << 28) != 0);
        set("fma3", r.ecx & (1 << 12) != 0);
        set("rdrand", r.ecx & (1 << 30) != 0);
        let e = r.eax;
        family = (((e >> 8) & 0xF) + ((e >> 20) & 0xFF)) as i64;
        model = (((e >> 4) & 0xF) + (((e >> 16) & 0xF) << 4)) as i64;
        stepping = (e & 0xF) as i64;
    }
    if n_ids >= 7 {
        let r = unsafe { std::arch::x86_64::__cpuid_count(7, 0) };
        set("avx2", r.ebx & (1 << 5) != 0);
        set("bmi1", r.ebx & (1 << 3) != 0);
        set("bmi2", r.ebx & (1 << 8) != 0);
        set("adx", r.ebx & (1 << 19) != 0);
        set("sha", r.ebx & (1 << 29) != 0);
        set("avx512f", r.ebx & (1 << 16) != 0);
        set("avx512cd", r.ebx & (1 << 28) != 0);
        set("avx512vl", r.ebx & (1 << 31) != 0);
        set("avx512bw", r.ebx & (1 << 30) != 0);
        set("avx512dq", r.ebx & (1 << 17) != 0);
        set("avx512ifma", r.ebx & (1 << 21) != 0);
        set("avx512vbmi", r.ecx & (1 << 1) != 0);
        set("vaes", r.ecx & (1 << 9) != 0);
    }
    if n_ex >= 0x8000_0001 {
        let r = unsafe { __cpuid(0x8000_0001) };
        set("x64", r.edx & (1 << 29) != 0);
        set("abm", r.ecx & (1 << 5) != 0);
        set("sse4a", r.ecx & (1 << 6) != 0);
        set("fma4", r.ecx & (1 << 16) != 0);
        set("xop", r.ecx & (1 << 11) != 0);
    }
    (family, model, stepping, vendor)
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "x86")))]
fn cpuid_features(_features: &mut BTreeMap<String, bool>) -> (i64, i64, i64, String) {
    (0, 0, 0, String::new())
}

#[cfg(target_os = "linux")]
fn linux_allowed_cpus() -> Option<HashSet<usize>> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = s.lines().find(|l| l.starts_with("Cpus_allowed_list:"))?;
    let list = line.split(':').nth(1)?.trim();
    let mut set = HashSet::new();
    for part in list.split(',') {
        if let Some((a, b)) = part.split_once('-') {
            let (a, b) = (a.trim().parse::<usize>().ok()?, b.trim().parse::<usize>().ok()?);
            for i in a..=b {
                set.insert(i);
            }
        } else if let Ok(x) = part.trim().parse::<usize>() {
            set.insert(x);
        }
    }
    if set.is_empty() {
        None
    } else {
        Some(set)
    }
}

/// CPU detection
pub fn detect_cpu(base: &Path) -> CpuInfo {
    let mut info = CpuInfo::default();
    let (family, model, stepping, cpuid_vendor) = cpuid_features(&mut info.Features);
    info.Family = family;
    info.Model = model;
    info.Stepping = stepping;

    let sys = sysinfo::System::new_with_specifics(sysinfo::RefreshKind::new().with_cpu(sysinfo::CpuRefreshKind::everything()));
    if let Some(c) = sys.cpus().first() {
        info.Name = c.brand().trim().to_string();
        info.Manufacturer = c.vendor_id().trim().to_string();
        info.MaxClockSpeed = c.frequency() as i64;
    }
    if info.Manufacturer.is_empty() {
        info.Manufacturer = cpuid_vendor;
    }
    info.Threads = sys.cpus().len().max(1);
    info.Cores = sys.physical_core_count().unwrap_or(info.Threads).max(1);
    info.PhysicalCPUs = 1;

    #[cfg(target_os = "linux")]
    {
        if let Ok(ci) = std::fs::read_to_string("/proc/cpuinfo") {
            let field = |name: &str| -> String {
                ci.lines()
                    .find(|l| l.split(':').next().map(|k| k.trim() == name).unwrap_or(false))
                    .and_then(|l| l.split_once(':').map(|x| x.1.trim().to_string()))
                    .unwrap_or_default()
            };
            if info.Name.is_empty() {
                info.Name = field("model name");
            }
            if info.Manufacturer.is_empty() {
                info.Manufacturer = field("vendor_id");
            }
            let cache = field("cache size");
            if !cache.is_empty() {
                info.L3CacheSize = (util::convert_from_bytes(&cache.replace(" KB", "k")) / 1048576.0).round() as i64;
            }
            for flags in [field("flags"), field("Features")] {
                for f in flags.split_whitespace() {
                    let k: String = f.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
                    if !k.is_empty() {
                        info.Features.insert(k, true);
                    }
                }
            }
            if info.Family == 0 {
                info.Family = field("cpu family").parse().unwrap_or(0);
                info.Model = field("model").parse().unwrap_or(0);
                info.Stepping = field("stepping").parse().unwrap_or(0);
            }
            let implementer = field("CPU implementer");
            if !implementer.is_empty() {
                info.Features.insert("arm".into(), true);
                let armdb = read_json(base, "armdb.json");
                let imp = i64::from_str_radix(implementer.trim_start_matches("0x"), 16).unwrap_or(0);
                let m = jsonx::s(jsonx::get(jsonx::get(&armdb, "implementers"), &imp.to_string()));
                if !m.is_empty() {
                    info.Manufacturer = m;
                }
                let part = i64::from_str_radix(field("CPU part").trim_start_matches("0x"), 16).unwrap_or(0);
                let pn = jsonx::s(jsonx::get(jsonx::get(jsonx::get(&armdb, "parts"), &imp.to_string()), &part.to_string()));
                if info.Name.is_empty() || info.Name == "Unknown" {
                    info.Name = if pn.is_empty() { "Unknown".into() } else { pn };
                }
            }
        }
        // topology: one thread per (socket, core) is a real core
        let mut topo: Vec<(usize, usize, usize, usize)> = Vec::new(); // socket, core, cpu, thread rank
        if let Ok(rd) = std::fs::read_dir("/sys/devices/system/cpu") {
            let re = Regex::new(r"^cpu(\d+)$").unwrap();
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                let Some(c) = re.captures(&n) else { continue };
                let cpu: usize = c[1].parse().unwrap_or(0);
                let p = e.path();
                let online = std::fs::read_to_string(p.join("online")).map(|s| s.trim() == "1").unwrap_or(true);
                if !online {
                    continue;
                }
                let rd = |f: &str| std::fs::read_to_string(p.join("topology").join(f)).ok().and_then(|s| s.trim().parse::<i64>().ok());
                let (Some(core), Some(sock)) = (rd("core_id"), rd("physical_package_id")) else { continue };
                topo.push((sock.max(0) as usize, core.max(0) as usize, cpu, 0));
            }
        }
        if let Some(allowed) = linux_allowed_cpus() {
            let before = topo.len();
            topo.retain(|t| allowed.contains(&t.2));
            if topo.len() < before && !topo.is_empty() {
                log_info!("CPU set limits RainbowMiner to {} of {} CPUs", topo.len(), before);
            }
        }
        if !topo.is_empty() {
            topo.sort();
            let mut real = Vec::new();
            let mut seen: HashSet<(usize, usize)> = HashSet::new();
            for t in &topo {
                if seen.insert((t.0, t.1)) {
                    real.push(t.2);
                }
            }
            real.sort_unstable();
            let mut all: Vec<usize> = topo.iter().map(|t| t.2).collect();
            all.sort_unstable();
            info.ThreadList = all.iter().filter(|c| !real.contains(c)).copied().collect();
            info.Cores = real.len();
            info.Threads = all.len();
            info.PhysicalCPUs = topo.iter().map(|t| t.0).collect::<HashSet<_>>().len().max(1);
            info.RealCores = real;
        }
    }

    if info.RealCores.is_empty() {
        // Windows style: hyperthreads are interleaved
        if info.Threads > info.Cores && info.Cores > 0 {
            let mult = (info.Threads / info.Cores).max(1);
            info.RealCores = (0..info.Threads).filter(|x| x % mult == 0).collect();
            info.ThreadList = (0..info.Threads).filter(|x| x % mult != 0).collect();
        } else {
            info.RealCores = (0..info.Cores).collect();
        }
    }
    if cfg!(target_pointer_width = "64") && !cfg!(any(target_arch = "aarch64", target_arch = "arm")) {
        info.Features.insert("x64".into(), true);
    }
    if cfg!(any(target_arch = "aarch64", target_arch = "arm")) {
        info.Features.insert("arm".into(), true);
    }

    let m = info.Manufacturer.clone();
    info.Vendor = if Regex::new(r"(?i)AMD|Advanced Micro Devices").unwrap().is_match(&m) {
        "AMD".into()
    } else if m.contains("Hygon") {
        "HYGON".into()
    } else if m.contains("Intel") {
        "INTEL".into()
    } else {
        let v: String = m
            .to_uppercase()
            .replace("(R)", "")
            .replace("(TM)", "")
            .replace("(C)", "")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        if v.is_empty() {
            if info.has("arm") {
                "ARM".into()
            } else {
                "OTHER".into()
            }
        } else {
            v
        }
    };
    if info.Vendor == "ARM" {
        info.Features.insert("arm".into(), true);
    }
    let f = |k: &str, i: &CpuInfo| i.has(k);
    if f("avx512f", &info) && f("avx512vl", &info) && f("avx512dq", &info) && f("avx512bw", &info) {
        info.Features.insert("avx512".into(), true);
    }
    if f("aesni", &info) {
        info.Features.insert("aes".into(), true);
    }
    if f("shani", &info) {
        info.Features.insert("sha".into(), true);
    }
    if info.Vendor == "AMD" || info.Vendor == "HYGON" {
        let zen = match info.Family {
            0x17 => match info.Model {
                0x01 | 0x11 | 0x18 | 0x20 => Some("zen"),
                0x08 => Some("zenplus"),
                0x31 | 0x47 | 0x60 | 0x68 | 0x71 | 0x90 | 0x98 => Some("zen2"),
                _ => None,
            },
            0x18 => Some("zen"),
            0x19 => Some("zen3"),
            _ => None,
        }
        .or_else(|| {
            if f("avx2", &info) && f("sha", &info) && f("vaes", &info) {
                Some("zen3")
            } else if f("avx2", &info) && f("sha", &info) && f("aes", &info) {
                Some("zen")
            } else {
                None
            }
        });
        if let Some(z) = zen {
            let ok = f("avx2", &info) && f("sha", &info) && ((z == "zen3" && f("vaes", &info)) || (z != "zen3" && f("aes", &info)));
            if ok {
                info.Features.insert(format!("is{}", z), true);
            }
        }
    }
    info.IsRyzen = ["iszen", "iszenplus", "iszen2", "iszen3", "iszen4"].iter().any(|z| info.has(z));

    // TDP from Data/cpu-tdp.json
    if info.is_arm() {
        info.TDP = 8.0;
    } else {
        let tdp = read_json(base, "cpu-tdp.json");
        let mut found = None;
        if let Value::Object(m) = &tdp {
            for (k, v) in m {
                if let Ok(re) = Regex::new(&format!("(?i){}", k)) {
                    if re.is_match(info.Name.trim()) {
                        found = Some(jsonx::num(v));
                        break;
                    }
                }
            }
            if found.is_none() && !m.is_empty() {
                found = Some(m.values().map(jsonx::num).sum::<f64>() / m.len() as f64);
            }
        }
        info.TDP = found.unwrap_or(65.0);
    }
    info
}

// ----------------------------------------------------------------------------------------
// detection of all devices (Get-Device -Refresh)
// ----------------------------------------------------------------------------------------

pub struct DetectOptions {
    pub base: PathBuf,
    pub opencl_platform_sorting: Vec<String>,
    pub nvsmi_path: String,
}

pub fn detect_devices(opts: &DetectOptions, cpu: &CpuInfo) -> Vec<Device> {
    let base = &opts.base;
    let mut lib_dirs = Vec::new();
    if cfg!(target_os = "linux") {
        lib_dirs.push(PathBuf::from("/opt/rainbowminer/lib"));
        lib_dirs.push(base.join("IncludesLinux").join("lib"));
    }
    let smi = nvidia_smi_path(base, &opts.nvsmi_path);
    let mut platforms = match opencl::enumerate(&lib_dirs) {
        Ok(p) => p,
        Err(e) => {
            log_info!("OpenCL detection failed: {}", e);
            Vec::new()
        }
    };
    let has_vendor_gpu = |pl: &[opencl::ClPlatform], v: &str| pl.iter().any(|p| p.vendor == v && p.devices.iter().any(|d| d.dtype == "Gpu"));
    if !has_vendor_gpu(&platforms, "NVIDIA") {
        if let Some(s) = &smi {
            if let Some(mut p) = nvidia_smi_platform(s) {
                log_info!("CUDA found: configured {} device(s) via nvidia-smi", p.devices.len());
                p.platform_id = platforms.len();
                platforms.push(p);
            }
        }
    }
    let fallback = if cfg!(windows) { windows_gpus() } else { sysfs_gpus(base) };
    for v in ["AMD", "INTEL"] {
        if !has_vendor_gpu(&platforms, v) {
            if let Some(mut p) = sysfs_platform(&fallback, v) {
                log_info!("{} GPU(s) found without OpenCL runtime", v);
                p.platform_id = platforms.len();
                platforms.push(p);
            }
        }
    }

    let known = ["AMD", "INTEL", "NVIDIA"];
    let mut devices: Vec<Device> = Vec::new();
    let mut index = 0usize;
    let mut platformid_index: HashMap<usize, usize> = HashMap::new();
    let mut type_platformid_index: HashMap<(String, usize), usize> = HashMap::new();
    let mut vendor_index: HashMap<String, usize> = HashMap::new();
    let mut type_index: HashMap<String, usize> = HashMap::new();
    let mut type_codec_index: HashMap<(String, String), usize> = HashMap::new();
    let mut type_mineable_index: HashMap<String, usize> = HashMap::new();
    let mut type_vendor_index: HashMap<(String, String), usize> = HashMap::new();
    let mut pci_seen: HashSet<String> = HashSet::new();
    let mut amd_models: HashMap<String, i64> = HashMap::new();
    let mut models_ex: Vec<String> = Vec::new();
    let driver_lhr_removed = util::Version(vec![522, 25]);

    for pl in &platforms {
        for d in &pl.devices {
            let dtype = d.dtype.clone();
            let vlow = d.vendor.to_lowercase();
            let vendor_name = if vlow.contains("nvidia") {
                "NVIDIA".to_string()
            } else if vlow.contains("advanced micro") || vlow.contains("amd") {
                "AMD".to_string()
            } else if vlow.contains("intel") {
                "INTEL".to_string()
            } else {
                d.vendor.clone()
            };
            let mut name = normalize_device_name(&d.name, &vendor_name);
            let mut sub_id = String::new();
            let mut cl = OpenClInfo {
                DeviceIndex: d.device_index,
                Name: d.name.clone(),
                Architecture: d.architecture.clone(),
                Type: dtype.clone(),
                Vendor: d.vendor.clone(),
                GlobalMemSize: d.global_mem_size,
                GlobalMemSizeGB: util::round_to(d.global_mem_size as f64 / 1073741824.0, 0) as i64,
                MaxMemAllocSize: d.max_mem_alloc_size,
                MaxComputeUnits: d.max_compute_units,
                PlatformVersion: d.platform_version.clone(),
                DriverVersion: d.driver_version.clone(),
                PCIBusId: d.pci_bus_id.clone(),
                DeviceCapability: d.device_capability.clone(),
                CardId: -1,
            };
            let mut card_id = -1i64;
            if vendor_name == "AMD" {
                if let Some(bus) = &d.pci_bus_id {
                    if let Some(g) = fallback.iter().find(|g| g.vendor == "AMD" && g.bus.eq_ignore_ascii_case(bus)) {
                        sub_id = g.device_id.clone();
                        card_id = g.card;
                        if (name.is_empty() || name.to_lowercase().starts_with("gfx")) && !g.name.is_empty() {
                            name = normalize_device_name(&g.name, "AMD");
                        }
                    }
                }
                if sub_id == "687F" || name == "Radeon RX Vega" || name == "gfx900" {
                    if d.max_compute_units == 56 {
                        name = "Radeon Vega 56".into();
                    } else if d.max_compute_units == 64 {
                        name = "Radeon Vega 64".into();
                    }
                } else if name == "gfx906" || name == "gfx907" {
                    name = "Radeon VII".into();
                } else if name == "gfx1010" {
                    name = "Radeon RX 5700 XT".into();
                }
                cl.Architecture = match Regex::new(r"^(gfx\d+)").unwrap().captures(&cl.Architecture) {
                    Some(c) => c[1].to_string(),
                    None => cl.Architecture.split(':').next().unwrap_or("").chars().filter(|c| c.is_ascii_alphanumeric()).collect(),
                };
            } else if vendor_name == "INTEL" {
                if let Some(c) = Regex::new(r"\[0x([0-9A-Fa-f]{4})\]").unwrap().captures(&name) {
                    let db = read_json(base, "intel-cards.json");
                    let n = jsonx::s(jsonx::get(&db, &c[1].to_lowercase()));
                    if !n.is_empty() {
                        name = normalize_device_name(&n, "INTEL");
                    }
                }
            } else if vendor_name == "NVIDIA" {
                if let Some(bus) = &d.pci_bus_id {
                    if let Some(g) = fallback.iter().find(|g| g.vendor == "NVIDIA" && g.bus.eq_ignore_ascii_case(bus)) {
                        sub_id = g.device_id.clone();
                    }
                }
            }
            let mut model = model_from_name(&name);
            if model.is_empty() {
                if !cl.Architecture.is_empty() {
                    model = cl.Architecture.clone();
                    name = format!("{}{}{}", name, if name.is_empty() { "" } else { " " }, model);
                } else {
                    model = "Unknown".into();
                    name = format!("{}{}{}", name, if name.is_empty() { "" } else { " " }, model);
                }
            }
            let codec = if vendor_name == "NVIDIA" {
                cl.Architecture = nvidia_architecture(base, &model, cl.DeviceCapability.as_deref().unwrap_or(""));
                "CUDA"
            } else {
                if vendor_name == "AMD" {
                    cl.DeviceCapability = Some(amd_compute_capability(base, &model, &cl.Architecture));
                }
                "OpenCL"
            };

            let tp = type_platformid_index.get(&(dtype.clone(), pl.platform_id)).copied().unwrap_or(0);
            let tc = type_codec_index.get(&(dtype.clone(), codec.to_string())).copied().unwrap_or(0);
            let tv = type_vendor_index.get(&(dtype.clone(), d.vendor.clone())).copied().unwrap_or(0);
            let mut dev = Device {
                Name: String::new(),
                Index: index,
                PlatformId: pl.platform_id,
                PlatformId_Index: platformid_index.get(&pl.platform_id).copied().unwrap_or(0),
                Type_PlatformId_Index: tp,
                Platform_Vendor: pl.vendor.clone(),
                Vendor: vendor_name.clone(),
                Vendor_Name: d.vendor.clone(),
                Vendor_Index: vendor_index.get(&d.vendor).copied().unwrap_or(0),
                Type: dtype.clone(),
                Type_Index: type_index.get(&dtype).copied().unwrap_or(0),
                Type_Codec_Index: tc,
                Type_Mineable_Index: type_mineable_index.get(&dtype).copied().unwrap_or(0),
                Type_Vendor_Index: tv,
                OpenCL: cl,
                Codec: codec.into(),
                Model: model.clone(),
                Model_Base: model.clone(),
                Model_Name: name.clone(),
                CardId: card_id,
                SubId: sub_id,
                ..Default::default()
            };
            let mut keep = true;
            if !dtype.eq_ignore_ascii_case("Cpu") {
                dev.Name = format!("{}#{:02}", dtype.to_uppercase(), dev.Type_Index);
                if !models_ex.contains(&dev.Model) {
                    let gb = dev.OpenCL.GlobalMemSizeGB;
                    match amd_models.get(&dev.Model) {
                        Some(g) if *g != gb => models_ex.push(dev.Model.clone()),
                        _ => {
                            amd_models.insert(dev.Model.clone(), gb);
                        }
                    }
                }
                let bus00 = dev.OpenCL.PCIBusId.as_deref().map(|b| b.starts_with("00:")).unwrap_or(false);
                if vendor_name == "NVIDIA" || vendor_name == "AMD" || (vendor_name == "INTEL" && !bus00) {
                    *type_mineable_index.entry(dtype.clone()).or_default() += 1;
                }
                if let Some(b) = dev.OpenCL.PCIBusId.clone() {
                    let b = b.to_uppercase();
                    dev.BusId = Some(b.clone());
                    if pci_seen.contains(&b) {
                        keep = false;
                    } else {
                        pci_seen.insert(b);
                    }
                }
                if keep {
                    if vendor_name == "NVIDIA" {
                        let dv = util::get_version(&dev.OpenCL.DriverVersion);
                        if dv.map(|v| v < driver_lhr_removed).unwrap_or(true) {
                            dev.IsLHR = Regex::new(r"^RTX30[1-8]0").unwrap().is_match(&model)
                                && !["2204", "2206", "2484", "2486"].contains(&dev.SubId.as_str());
                        }
                    }
                    devices.push(dev);
                    index += 1;
                }
            }
            if keep {
                *platformid_index.entry(pl.platform_id).or_default() += 1;
                *type_platformid_index.entry((dtype.clone(), pl.platform_id)).or_default() += 1;
                *vendor_index.entry(d.vendor.clone()).or_default() += 1;
                *type_index.entry(dtype.clone()).or_default() += 1;
                *type_codec_index.entry((dtype.clone(), codec.to_string())).or_default() += 1;
                *type_vendor_index.entry((dtype.clone(), d.vendor.clone())).or_default() += 1;
            }
        }
    }

    for m in &models_ex {
        for d in devices.iter_mut().filter(|d| &d.Model == m) {
            let gb = format!("{}GB", d.OpenCL.GlobalMemSizeGB);
            d.Model = format!("{}{}", d.Model, gb);
            d.Model_Base = format!("{}{}", d.Model_Base, gb);
            d.Model_Name = format!("{} {}", d.Model_Name, gb);
        }
    }

    // re-index in case the OpenCL platforms have shifted positions
    if !platforms.is_empty() {
        let mut order: Vec<String> = opts.opencl_platform_sorting.clone();
        if order.is_empty() {
            let v = read_json(base, "openclplatforms.json");
            order = jsonx::str_list(&v);
        }
        let mut current: Vec<(bool, usize, String)> = platforms.iter().map(|p| (!known.contains(&p.vendor.as_str()), p.platform_id, p.vendor.clone())).collect();
        current.sort();
        let mut changed = false;
        for (_, _, v) in current {
            if !order.contains(&v) {
                order.push(v);
                changed = true;
            }
        }
        if changed && opts.opencl_platform_sorting.is_empty() {
            let _ = crate::config::write_json(&base.join("Data").join("openclplatforms.json"), &json!(order));
        }
        devices.sort_by(|a, b| {
            let pa = order.iter().position(|x| *x == a.Platform_Vendor).unwrap_or(usize::MAX);
            let pb = order.iter().position(|x| *x == b.Platform_Vendor).unwrap_or(usize::MAX);
            pa.cmp(&pb).then(a.Index.cmp(&b.Index))
        });
        let mut resorted = false;
        for (i, d) in devices.iter_mut().enumerate() {
            if d.Index != i {
                resorted = true;
                d.Index = i;
                d.Name = format!("{}#{:02}", d.Type.to_uppercase(), i);
            }
        }
        if resorted {
            log_info!("OpenCL platforms have changed from initial run. Resorting indices.");
        }
    }

    // CPU devices
    let physical = if cfg!(target_os = "linux") { 1 } else { cpu.PhysicalCPUs.max(1) };
    for ci in 0..physical {
        let dev = Device {
            Name: format!("CPU#{:02}", ci),
            Index: index,
            Vendor: cpu.Vendor.clone(),
            Vendor_Name: cpu.Manufacturer.clone(),
            Type_PlatformId_Index: ci,
            Type_Vendor_Index: ci,
            Type: "Cpu".into(),
            Type_Index: ci,
            Type_Mineable_Index: ci,
            Type_Codec_Index: ci,
            Model: "CPU".into(),
            Model_Base: "CPU".into(),
            Model_Name: cpu.Name.clone(),
            Codec: String::new(),
            Features: cpu.Features.keys().cloned().collect(),
            Data: DeviceData {
                Cores: Some(cpu.Cores / physical),
                Threads: Some(cpu.Threads / physical),
                CacheL3: Some(cpu.L3CacheSize),
                ..Default::default()
            },
            ..Default::default()
        };
        devices.push(dev);
        index += 1;
    }

    // sort order by PCI bus id
    let mut order: Vec<usize> = (0..devices.len()).collect();
    let busnum = |d: &Device| -> u64 {
        let h: String = d.BusId.clone().unwrap_or_default().chars().filter(|c| c.is_ascii_hexdigit()).collect();
        u64::from_str_radix(&format!("0{}", h), 16).unwrap_or(0)
    };
    order.sort_by(|a, b| busnum(&devices[*a]).cmp(&busnum(&devices[*b])).then(devices[*a].Index.cmp(&devices[*b].Index)));
    let mut bi = 0usize;
    let mut bt: HashMap<String, usize> = HashMap::new();
    let mut btc: HashMap<(String, String), usize> = HashMap::new();
    let mut btv: HashMap<(String, String), usize> = HashMap::new();
    let mut btm: HashMap<String, usize> = HashMap::new();
    let mut bv: HashMap<String, usize> = HashMap::new();
    for i in order {
        let d = &mut devices[i];
        d.BusId_Index = bi;
        bi += 1;
        d.BusId_Type_Index = *bt.get(&d.Type).unwrap_or(&0);
        d.BusId_Type_Codec_Index = *btc.get(&(d.Type.clone(), d.Codec.clone())).unwrap_or(&0);
        d.BusId_Type_Vendor_Index = *btv.get(&(d.Type.clone(), d.Vendor.clone())).unwrap_or(&0);
        d.BusId_Type_Mineable_Index = *btm.get(&d.Type).unwrap_or(&0);
        d.BusId_Vendor_Index = *bv.get(&d.Vendor).unwrap_or(&0);
        *bt.entry(d.Type.clone()).or_default() += 1;
        *btc.entry((d.Type.clone(), d.Codec.clone())).or_default() += 1;
        *btv.entry((d.Type.clone(), d.Vendor.clone())).or_default() += 1;
        *bv.entry(d.Vendor.clone()).or_default() += 1;
        let bus00 = d.BusId.as_deref().map(|b| b.starts_with("00:")).unwrap_or(false);
        if d.Vendor == "AMD" || d.Vendor == "NVIDIA" || (d.Vendor == "INTEL" && !bus00) {
            *btm.entry(d.Type.clone()).or_default() += 1;
        }
    }
    devices
}

// ----------------------------------------------------------------------------------------
// selection (Get-Device -Name / -ExcludeName with Data/devices.json)
// ----------------------------------------------------------------------------------------

/// PowerShell -like (case insensitive, * and ? wildcards)
pub fn like(s: &str, pat: &str) -> bool {
    let s: Vec<char> = s.to_lowercase().chars().collect();
    let p: Vec<char> = pat.to_lowercase().chars().collect();
    fn rec(s: &[char], p: &[char]) -> bool {
        match (p.first(), s.first()) {
            (None, None) => true,
            (None, Some(_)) => false,
            (Some('*'), _) => rec(s, &p[1..]) || (!s.is_empty() && rec(&s[1..], p)),
            (Some('?'), Some(_)) => rec(&s[1..], &p[1..]),
            (Some(a), Some(b)) if a == b => rec(&s[1..], &p[1..]),
            _ => false,
        }
    }
    rec(&s, &p)
}

fn name_templates(base: &Path, names: &[String]) -> Vec<IndexMap<String, String>> {
    let list = read_json(base, "devices.json");
    let mut out = Vec::new();
    for n in names {
        let parts: Vec<&str> = n.split('#').take(3).collect();
        let mut split = vec!["*".to_string(), "*".to_string(), "*".to_string()];
        for (i, p) in parts.iter().enumerate() {
            split[i] = if i > 0 { p.trim().parse::<i64>().map(|x| x.to_string()).unwrap_or_else(|_| p.to_string()) } else { p.to_string() };
        }
        let t = jsonx::get(&list, &split[0]);
        if let Value::Object(m) = t {
            let mut tm = IndexMap::new();
            for (k, v) in m {
                let val = jsonx::s(v).replace("{0}", &split[0]).replace("{1}", &split[1]).replace("{2}", &split[2]);
                tm.insert(k.clone(), val);
            }
            out.push(tm);
        }
    }
    out
}

fn matches_template(d: &Device, t: &IndexMap<String, String>) -> bool {
    t.iter().all(|(k, pat)| like(&d.prop(k), pat))
}

/// Get-Device $Name $ExcludeName
pub fn select_devices(base: &Path, all: &[Device], names: &[String], exclude: &[String]) -> Vec<Device> {
    let inc = name_templates(base, names);
    let exc = name_templates(base, exclude);
    all.iter()
        .filter(|d| {
            (names.is_empty()
                || inc.iter().any(|t| matches_template(d, t))
                || names.iter().any(|n| like(&d.Model, n) || like(&d.Model_Name, n)))
                && (exclude.is_empty()
                    || (!exc.iter().any(|t| matches_template(d, t)) && !exclude.iter().any(|n| like(&d.Model, n) || like(&d.Model_Name, n))))
        })
        .cloned()
        .collect()
}

// ----------------------------------------------------------------------------------------
// device cache with combos ($Global:DeviceCache)
// ----------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct DeviceCache {
    pub all_devices: Vec<Device>,
    pub devices: Vec<Device>,
    /// "AMD","INTEL","NVIDIA","CPU" -> devices incl. combo copies
    pub by_types: IndexMap<String, Vec<Device>>,
    /// type -> "Model1-Model2" (all models of a vendor)
    pub full_combo_models: HashMap<String, String>,
    /// model -> sorted device names
    pub device_names: HashMap<String, Vec<String>>,
    /// model -> vendor type (stats prefix)
    pub devices_to_vendors: HashMap<String, String>,
    pub device_combos: Vec<String>,
    pub devices_names: Vec<String>,
    /// all possible subsets per type (for combos.config.txt)
    pub subsets: HashMap<String, Vec<String>>,
}

/// Get-DeviceSubsets: all combinations (size > 1) of the distinct models
fn device_subsets(devs: &[Device]) -> Vec<(Vec<String>, Vec<String>)> {
    let mut models: Vec<(String, String)> = Vec::new();
    for d in devs {
        if !models.iter().any(|m| m.0 == d.Model) {
            models.push((d.Model.clone(), d.Model_Name.clone()));
        }
    }
    let n = models.len();
    let mut out = Vec::new();
    if n < 2 || n > 12 {
        return out;
    }
    for mask in 1u32..(1 << n) {
        if mask.count_ones() < 2 {
            continue;
        }
        let mut sel: Vec<(String, String)> = (0..n).filter(|i| mask & (1 << i) != 0).map(|i| models[i].clone()).collect();
        sel.sort_by(|a, b| a.0.cmp(&b.0));
        out.push((sel.iter().map(|x| x.0.clone()).collect(), sel.iter().map(|x| x.1.clone()).collect()));
    }
    out
}

impl DeviceCache {
    pub fn build(all: Vec<Device>, selected: Vec<Device>, mining_mode: &str, combos_cfg: &HashMap<String, HashMap<String, bool>>) -> DeviceCache {
        let mut c = DeviceCache {
            all_devices: all,
            devices: selected,
            ..Default::default()
        };
        for t in ["AMD", "INTEL", "NVIDIA", "CPU"] {
            let v: Vec<Device> = c.devices.iter().filter(|d| d.type_key() == t).cloned().collect();
            c.by_types.insert(t.to_string(), v);
        }
        let mut combos: HashMap<String, Vec<Device>> = HashMap::new();
        for t in ["AMD", "INTEL", "NVIDIA", "CPU"] {
            let devs = c.by_types[t].clone();
            let mut models: Vec<String> = devs.iter().map(|d| d.Model.clone()).collect();
            models.sort();
            models.dedup();
            let full = models.join("-");
            c.full_combo_models.insert(t.to_string(), full.clone());
            let mut list = Vec::new();
            let subsets = device_subsets(&devs);
            c.subsets.insert(t.to_string(), subsets.iter().map(|s| s.0.join("-")).collect());
            for (ms, names) in subsets {
                let key = ms.join("-");
                let enabled = (key == full) || combos_cfg.get(t).and_then(|m| m.get(&key.to_lowercase())).copied().unwrap_or(false);
                if !enabled {
                    continue;
                }
                for d in devs.iter().filter(|d| ms.contains(&d.Model)) {
                    let mut x = d.clone();
                    x.Model = key.clone();
                    x.Model_Name = names.join("+");
                    list.push(x);
                }
            }
            for m in models.iter().chain(list.iter().map(|d| &d.Model)) {
                c.devices_to_vendors.insert(m.clone(), t.to_string());
            }
            combos.insert(t.to_string(), list);
        }
        if mining_mode == "legacy" {
            for t in ["AMD", "INTEL", "NVIDIA", "CPU"] {
                let full = c.full_combo_models[t].clone();
                if full.contains('-') {
                    let v: Vec<Device> = combos[t].iter().filter(|d| d.Model == full).cloned().collect();
                    c.by_types.insert(t.to_string(), v);
                }
            }
        } else if mining_mode == "combo" {
            for t in ["AMD", "INTEL", "NVIDIA", "CPU"] {
                let add = combos[t].clone();
                c.by_types.get_mut(t).unwrap().extend(add);
            }
        }
        for t in ["AMD", "INTEL", "NVIDIA", "CPU"] {
            let mut groups: IndexMap<String, Vec<String>> = IndexMap::new();
            for d in &c.by_types[t] {
                groups.entry(d.Model.clone()).or_default().push(d.Name.clone());
            }
            for (m, mut names) in groups {
                names.sort();
                names.dedup();
                c.device_names.insert(m, names);
            }
        }
        let mut dc: Vec<String> = Vec::new();
        for t in ["AMD", "INTEL", "NVIDIA", "CPU"] {
            for d in &c.by_types[t] {
                if !dc.contains(&d.Model) {
                    dc.push(d.Model.clone());
                }
            }
        }
        dc.sort();
        c.device_combos = dc;
        let mut dn: Vec<String> = c.devices.iter().map(|d| d.Name.clone()).collect();
        dn.sort();
        dn.dedup();
        c.devices_names = dn;
        c
    }

    pub fn vendor_of_model(&self, model: &str) -> String {
        self.devices_to_vendors.get(model).cloned().unwrap_or_default()
    }

    /// distinct models of a vendor type, in order (Select-Object Vendor, Model -Unique)
    pub fn models_of(&self, t: &str) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for d in self.by_types.get(t).map(|x| x.as_slice()).unwrap_or(&[]) {
            if !v.contains(&d.Model) {
                v.push(d.Model.clone());
            }
        }
        v
    }

    pub fn devices_of(&self, t: &str, model: &str) -> Vec<&Device> {
        self.by_types
            .get(t)
            .map(|v| v.iter().filter(|d| d.Model == model).collect())
            .unwrap_or_default()
    }

    pub fn has_type(&self, t: &str) -> bool {
        self.by_types.get(t).map(|v| !v.is_empty()).unwrap_or(false)
    }

    pub fn device(&self, name: &str) -> Option<&Device> {
        self.devices.iter().find(|d| d.Name.eq_ignore_ascii_case(name))
    }
}

// ----------------------------------------------------------------------------------------
// live sensor data (Update-DeviceInformation)
// ----------------------------------------------------------------------------------------

struct SensorState {
    sys: sysinfo::System,
}

static SENSORS: LazyLock<Mutex<SensorState>> = LazyLock::new(|| {
    Mutex::new(SensorState {
        sys: sysinfo::System::new(),
    })
});

#[cfg(target_os = "linux")]
fn amd_sysfs_update(d: &mut Device) {
    let Some(bus) = d.BusId.clone() else { return };
    let Ok(rd) = std::fs::read_dir("/sys/class/drm") else { return };
    for e in rd.flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        if !n.starts_with("card") || n.contains('-') {
            continue;
        }
        let dev = e.path().join("device");
        let Some(fname) = std::fs::canonicalize(&dev).ok().and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_uppercase())) else {
            continue;
        };
        if !fname.contains(&format!(":{}.", bus.to_uppercase())) {
            continue;
        }
        let read = |p: PathBuf| std::fs::read_to_string(p).ok().and_then(|s| s.trim().parse::<f64>().ok());
        d.Data.Utilization = read(dev.join("gpu_busy_percent")).unwrap_or(0.0);
        if let Ok(hw) = std::fs::read_dir(dev.join("hwmon")) {
            for h in hw.flatten() {
                let hp = h.path();
                if let Some(t) = read(hp.join("temp1_input")) {
                    d.Data.Temperature = t / 1000.0;
                }
                if let Some(p) = read(hp.join("power1_average")).or_else(|| read(hp.join("power1_input"))) {
                    d.Data.PowerDraw = p / 1e6;
                }
                if let Some(cap) = read(hp.join("power1_cap")) {
                    d.Data.PowerLimit = cap / 1e6;
                }
                if let Some(def) = read(hp.join("power1_cap_default")) {
                    d.Data.PowerDefaultLimit = def / 1e6;
                }
                if let Some(mx) = read(hp.join("power1_cap_max")) {
                    d.Data.PowerMaxLimit = mx / 1e6;
                }
                if let (Some(pwm), Some(max)) = (read(hp.join("pwm1")), read(hp.join("pwm1_max"))) {
                    if max > 0.0 {
                        d.Data.FanSpeed = (pwm / max * 100.0).round();
                    }
                }
                if let Some(f) = read(hp.join("freq1_input")) {
                    d.Data.Clock = f / 1e6;
                }
                if let Some(f) = read(hp.join("freq2_input")) {
                    d.Data.ClockMem = f / 1e6;
                }
            }
        }
        if d.Data.PowerDefaultLimit > 0.0 {
            d.Data.PowerLimitPercent = (d.Data.PowerLimit * 100.0 / d.Data.PowerDefaultLimit).floor();
        }
        d.Data.Method = "sysfs".into();
        return;
    }
}

#[cfg(not(target_os = "linux"))]
fn amd_sysfs_update(_d: &mut Device) {}

/// Update-DeviceInformation: refresh the sensors of the named devices
pub fn update_device_information(base: &Path, devices: &mut [Device], names: &[String], power_adjust: &HashMap<String, f64>, nvsmi_path: &str, cpu_tdp: f64) {
    let adjust = |model: &str| -> f64 { power_adjust.get(&model.to_lowercase()).copied().filter(|x| *x > 0.0).unwrap_or(100.0) };
    let want = |d: &Device| names.is_empty() || names.iter().any(|n| n.eq_ignore_ascii_case(&d.Name));

    // NVIDIA
    if devices.iter().any(|d| d.is_gpu() && d.Vendor == "NVIDIA" && want(d)) {
        if let Some(smi) = nvidia_smi_path(base, nvsmi_path) {
            let tdp = read_json(base, "nvidia-cards-tdp.json");
            let rows = invoke_nvidia_smi(
                &smi,
                &[
                    "index", "pci.bus_id", "utilization.gpu", "utilization.memory", "temperature.gpu", "power.draw", "power.limit", "fan.speed", "pstate",
                    "clocks.current.graphics", "clocks.current.memory", "power.max_limit", "power.default_limit",
                ],
            );
            let re_bus = Regex::new(r"(?i):([0-9A-F]{2}:[0-9A-F]{2})").unwrap();
            for (i, r) in rows.iter().enumerate() {
                let g = |k: &str| r.get(k).cloned().unwrap_or(Value::Null);
                let bus = re_bus.captures(&jsonx::s(&g("pci_bus_id"))).map(|c| c[1].to_uppercase());
                for d in devices.iter_mut().filter(|d| d.is_gpu() && d.Vendor == "NVIDIA") {
                    let hit = match (&bus, &d.BusId) {
                        (Some(b), Some(db)) => b.eq_ignore_ascii_case(db),
                        _ => d.Type_Vendor_Index == i,
                    };
                    if !hit || !want(d) {
                        continue;
                    }
                    d.Data.Utilization = jsonx::num_opt(&g("utilization_gpu")).unwrap_or(100.0);
                    d.Data.UtilizationMem = jsonx::num(&g("utilization_memory"));
                    d.Data.Temperature = jsonx::num(&g("temperature_gpu"));
                    d.Data.PowerDraw = jsonx::num(&g("power_draw"));
                    d.Data.PowerLimit = jsonx::num(&g("power_limit"));
                    d.Data.FanSpeed = jsonx::num(&g("fan_speed"));
                    d.Data.Pstate = jsonx::s(&g("pstate"));
                    d.Data.Clock = jsonx::num(&g("clocks_current_graphics"));
                    d.Data.ClockMem = jsonx::num(&g("clocks_current_memory"));
                    d.Data.PowerMaxLimit = jsonx::num(&g("power_max_limit"));
                    d.Data.PowerDefaultLimit = jsonx::num(&g("power_default_limit"));
                    d.Data.Method = "smi".into();
                    d.Data.PowerDrawEstimated = false;
                    if d.Data.PowerDefaultLimit > 0.0 {
                        d.Data.PowerLimitPercent = (d.Data.PowerLimit * 100.0 / d.Data.PowerDefaultLimit).floor();
                    }
                    let t = jsonx::num(jsonx::get(&tdp, &d.Model_Name));
                    if d.Data.PowerDraw <= 0.0 && t > 0.0 {
                        d.Data.PowerDraw = t * (d.Data.PowerLimitPercent / 100.0) * (d.Data.Utilization / 100.0);
                        d.Data.PowerDrawEstimated = true;
                    }
                }
            }
        }
    }

    // AMD / INTEL
    let amd_tdp = read_json(base, "amd-cards-tdp.json");
    let intel_tdp = read_json(base, "intel-cards-tdp.json");
    for d in devices.iter_mut().filter(|d| d.is_gpu() && (d.Vendor == "AMD" || d.Vendor == "INTEL")) {
        if !want(d) {
            continue;
        }
        amd_sysfs_update(d);
        if d.Data.PowerDraw <= 0.0 {
            let db = if d.Vendor == "AMD" { &amd_tdp } else { &intel_tdp };
            let t = jsonx::num(jsonx::get(db, &d.Model_Name));
            if t > 0.0 {
                let util = if d.Data.Utilization > 0.0 { d.Data.Utilization } else { 100.0 };
                d.Data.PowerDraw = t * util / 100.0;
                d.Data.PowerDrawEstimated = true;
            }
        }
    }

    for d in devices.iter_mut().filter(|d| d.is_gpu() && want(d)) {
        d.DataMax.Clock = d.DataMax.Clock.max(d.Data.Clock);
        d.DataMax.ClockMem = d.DataMax.ClockMem.max(d.Data.ClockMem);
        d.DataMax.Temperature = d.DataMax.Temperature.max(d.Data.Temperature);
        d.DataMax.FanSpeed = d.DataMax.FanSpeed.max(d.Data.FanSpeed);
        d.Data.PowerDraw *= adjust(&d.Model) / 100.0;
        d.DataMax.PowerDraw = d.DataMax.PowerDraw.max(d.Data.PowerDraw);
    }

    // CPU
    if devices.iter().any(|d| d.is_cpu() && want(d)) {
        let mut st = SENSORS.lock().unwrap();
        st.sys.refresh_cpu_usage();
        let usage = st.sys.global_cpu_info().cpu_usage() as f64;
        let freq = st.sys.cpus().first().map(|c| c.frequency() as f64).unwrap_or(0.0);
        drop(st);
        for d in devices.iter_mut().filter(|d| d.is_cpu() && want(d)) {
            d.Data.Utilization = usage.round();
            d.Data.Clock = freq;
            let mut p = cpu_tdp * usage / 100.0;
            p *= adjust("CPU") / 100.0;
            d.Data.PowerDraw = util::round_to(p, 2);
            d.Data.Method = "tdp".into();
            d.Data.PowerDrawEstimated = true;
            d.DataMax.Clock = d.DataMax.Clock.max(d.Data.Clock);
            d.DataMax.Utilization = d.DataMax.Utilization.max(d.Data.Utilization);
            d.DataMax.PowerDraw = d.DataMax.PowerDraw.max(d.Data.PowerDraw);
            d.DataMax.Temperature = d.DataMax.Temperature.max(d.Data.Temperature);
        }
    }
}

/// Get-DevicePowerDraw
pub fn device_power_draw(devices: &[Device], names: &[String]) -> f64 {
    devices
        .iter()
        .filter(|d| names.is_empty() || names.iter().any(|n| n.eq_ignore_ascii_case(&d.Name)))
        .map(|d| d.Data.PowerDraw)
        .sum()
}

#[derive(Debug, Clone, Default)]
pub struct PowerState {
    pub devices: usize,
    pub is_real: bool,
    pub power_draw: f64,
    pub reference: f64,
    pub utilization: f64,
    pub is_idle: bool,
}

/// Get-DevicePowerState (stall detection)
pub fn device_power_state(base: &Path, devices: &[Device], names: &[String]) -> PowerState {
    let gpus: Vec<&Device> = devices
        .iter()
        .filter(|d| d.is_gpu() && (names.is_empty() || names.iter().any(|n| n.eq_ignore_ascii_case(&d.Name))))
        .collect();
    let mut st = PowerState {
        devices: gpus.len(),
        is_real: !gpus.is_empty(),
        ..Default::default()
    };
    for d in gpus {
        let mut reference = [d.Data.PowerDefaultLimit, d.Data.PowerLimit, d.Data.PowerMaxLimit].into_iter().find(|x| *x > 0.0).unwrap_or(0.0);
        if reference <= 0.0 {
            let file = match d.Vendor.as_str() {
                "NVIDIA" => "nvidia-cards-tdp.json",
                "AMD" => "amd-cards-tdp.json",
                _ => "intel-cards-tdp.json",
            };
            reference = jsonx::num(jsonx::get(&read_json(base, file), &d.Model_Name));
        }
        if reference <= 0.0 || d.Data.PowerDraw <= 0.0 || d.Data.PowerDrawEstimated {
            st.is_real = false;
        }
        st.power_draw += d.Data.PowerDraw;
        st.reference += reference;
        st.utilization = st.utilization.max(d.Data.Utilization);
    }
    st.is_idle = st.is_real && st.power_draw < st.reference * 0.12 && st.utilization <= 20.0;
    st
}

/// usable VRAM of a GPU in bytes (Test-VRAM's memo): GPUReservedVRAMGB, else static factors
pub fn usable_vram(d: &Device, reserved_cfg: &str) -> (f64, f64) {
    let mem = d.OpenCL.GlobalMemSize as f64;
    if let Ok(r) = reserved_cfg.trim().parse::<f64>() {
        return ((mem / 1073741824.0 - r).max(0.0) * 1073741824.0, 0.0);
    }
    if let Some(r) = d.ReservedVRAMGB {
        return ((mem / 1073741824.0 - r).max(0.0) * 1073741824.0, 0.0);
    }
    if cfg!(windows) && d.Vendor == "NVIDIA" {
        (mem * 0.865, 0.0)
    } else {
        (mem, 0.25)
    }
}

/// Test-VRAM
pub fn test_vram(d: &Device, min_mem_gb: f64, reserved_cfg: &str) -> bool {
    let (m, off) = usable_vram(d, reserved_cfg);
    m >= (min_mem_gb + off) * 1073741824.0
}

/// Get-DeviceModelName -Short
pub fn device_model_name_short(devices: &[Device], names: &[String]) -> Vec<String> {
    let re = Regex::new(r"(?i)geforce|radeon|intel|\(r\)").unwrap();
    let mut out: Vec<String> = Vec::new();
    for d in devices.iter().filter(|d| names.is_empty() || names.iter().any(|n| n.eq_ignore_ascii_case(&d.Name))) {
        let n = if d.is_cpu() { "CPU".to_string() } else { d.Model_Name.clone() };
        let n = re.replace_all(&n, "").trim().to_string();
        if !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(normalize_device_name("NVIDIA GeForce RTX 3080", "NVIDIA"), "GeForce RTX 3080");
        assert_eq!(model_from_name("GeForce RTX 3080"), "RTX3080");
        assert_eq!(normalize_device_name("AMD Radeon RX 580 Series", "AMD"), "Radeon RX 580");
        assert!(like("GPU", "gpu"));
        assert!(like("RTX3080", "RTX*"));
        assert!(!like("GTX1080", "RTX*"));
    }

    #[test]
    fn selection() {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
        let mk = |i: usize, vendor: &str, t: &str| Device {
            Name: format!("{}#{:02}", t.to_uppercase(), i),
            Index: i,
            Vendor: vendor.into(),
            Type: t.into(),
            Type_Index: i,
            Type_Vendor_Index: i,
            Model: "RTX3080".into(),
            ..Default::default()
        };
        let all = vec![mk(0, "NVIDIA", "Gpu"), mk(1, "NVIDIA", "Gpu"), mk(0, "INTEL", "Cpu")];
        let s = select_devices(&base, &all, &["gpu#01".into()], &[]);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].Name, "GPU#01");
        let s = select_devices(&base, &all, &["gpu".into(), "cpu".into()], &["nvidia#00".into()]);
        assert_eq!(s.len(), 2);
        let s = select_devices(&base, &all, &["RTX3080".into()], &[]);
        assert_eq!(s.len(), 3);
    }
}
