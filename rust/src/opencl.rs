//! Minimal OpenCL device enumeration (DotNet/OpenCL + Scripts/GetOpenCL.ps1), loaded at runtime
//! so that RainbowMiner also runs on machines without an OpenCL ICD.

use libloading::{Library, Symbol};
use std::ffi::c_void;

type ClInt = i32;
type ClUint = u32;
type ClPlatformId = *mut c_void;
type ClDeviceId = *mut c_void;

type FnGetPlatformIds = unsafe extern "C" fn(ClUint, *mut ClPlatformId, *mut ClUint) -> ClInt;
type FnGetPlatformInfo = unsafe extern "C" fn(ClPlatformId, ClUint, usize, *mut c_void, *mut usize) -> ClInt;
type FnGetDeviceIds = unsafe extern "C" fn(ClPlatformId, u64, ClUint, *mut ClDeviceId, *mut ClUint) -> ClInt;
type FnGetDeviceInfo = unsafe extern "C" fn(ClDeviceId, ClUint, usize, *mut c_void, *mut usize) -> ClInt;

const CL_PLATFORM_VERSION: ClUint = 0x0901;
const CL_PLATFORM_NAME: ClUint = 0x0902;
const CL_PLATFORM_VENDOR: ClUint = 0x0903;
const CL_DEVICE_TYPE_ALL: u64 = 0xFFFF_FFFF;
const CL_DEVICE_TYPE: ClUint = 0x1000;
const CL_DEVICE_MAX_COMPUTE_UNITS: ClUint = 0x1002;
const CL_DEVICE_MAX_MEM_ALLOC_SIZE: ClUint = 0x1010;
const CL_DEVICE_GLOBAL_MEM_SIZE: ClUint = 0x101F;
const CL_DEVICE_NAME: ClUint = 0x102B;
const CL_DEVICE_VENDOR: ClUint = 0x102C;
const CL_DRIVER_VERSION: ClUint = 0x102D;
const CL_DEVICE_COMPUTE_CAPABILITY_MAJOR_NV: ClUint = 0x4000;
const CL_DEVICE_COMPUTE_CAPABILITY_MINOR_NV: ClUint = 0x4001;
const CL_DEVICE_PCI_BUS_ID_NV: ClUint = 0x4008;
const CL_DEVICE_PCI_SLOT_ID_NV: ClUint = 0x4009;
const CL_DEVICE_TOPOLOGY_AMD: ClUint = 0x4037;
const CL_DEVICE_BOARD_NAME_AMD: ClUint = 0x4038;
const CL_DEVICE_PCI_BUS_INFO_KHR: ClUint = 0x410F;

#[derive(Debug, Clone, Default)]
pub struct ClDevice {
    pub device_index: usize,
    pub name: String,
    pub architecture: String,
    /// "Cpu", "Gpu", "Accelerator"
    pub dtype: String,
    pub vendor: String,
    pub global_mem_size: u64,
    pub max_mem_alloc_size: u64,
    pub max_compute_units: u32,
    pub platform_version: String,
    pub driver_version: String,
    pub pci_bus_id: Option<String>,
    pub device_capability: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ClPlatform {
    pub platform_id: usize,
    /// normalized: AMD, INTEL, NVIDIA or the stripped vendor string
    pub vendor: String,
    pub name: String,
    pub devices: Vec<ClDevice>,
}

struct Api<'a> {
    get_platform_info: Symbol<'a, FnGetPlatformInfo>,
    get_device_info: Symbol<'a, FnGetDeviceInfo>,
}

fn info_bytes(f: impl Fn(usize, *mut c_void, *mut usize) -> ClInt) -> Option<Vec<u8>> {
    let mut size = 0usize;
    if f(0, std::ptr::null_mut(), &mut size) != 0 || size == 0 {
        return None;
    }
    let mut buf = vec![0u8; size];
    if f(size, buf.as_mut_ptr() as *mut c_void, std::ptr::null_mut()) != 0 {
        return None;
    }
    Some(buf)
}

fn bytes_to_string(b: Vec<u8>) -> String {
    let end = b.iter().position(|c| *c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).trim().to_string()
}

impl<'a> Api<'a> {
    fn platform_str(&self, p: ClPlatformId, param: ClUint) -> String {
        info_bytes(|s, v, r| unsafe { (self.get_platform_info)(p, param, s, v, r) })
            .map(bytes_to_string)
            .unwrap_or_default()
    }
    fn device_bytes(&self, d: ClDeviceId, param: ClUint) -> Option<Vec<u8>> {
        info_bytes(|s, v, r| unsafe { (self.get_device_info)(d, param, s, v, r) })
    }
    fn device_str(&self, d: ClDeviceId, param: ClUint) -> String {
        self.device_bytes(d, param).map(bytes_to_string).unwrap_or_default()
    }
    fn device_u32(&self, d: ClDeviceId, param: ClUint) -> Option<u32> {
        let b = self.device_bytes(d, param)?;
        if b.len() >= 4 {
            Some(u32::from_ne_bytes([b[0], b[1], b[2], b[3]]))
        } else {
            None
        }
    }
    fn device_u64(&self, d: ClDeviceId, param: ClUint) -> Option<u64> {
        let b = self.device_bytes(d, param)?;
        if b.len() >= 8 {
            let mut a = [0u8; 8];
            a.copy_from_slice(&b[..8]);
            Some(u64::from_ne_bytes(a))
        } else if b.len() >= 4 {
            Some(u32::from_ne_bytes([b[0], b[1], b[2], b[3]]) as u64)
        } else {
            None
        }
    }
}

fn normalize_vendor(v: &str) -> String {
    let l = v.to_lowercase();
    if l.contains("advanced micro devices") || l.contains("amd") {
        "AMD".into()
    } else if l.contains("intel") {
        "INTEL".into()
    } else if l.contains("nvidia") {
        "NVIDIA".into()
    } else {
        v.replace("(R)", "")
            .replace("(TM)", "")
            .replace("(C)", "")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_uppercase()
    }
}

fn library_names() -> Vec<&'static str> {
    if cfg!(windows) {
        vec!["OpenCL.dll"]
    } else if cfg!(target_os = "macos") {
        vec!["/System/Library/Frameworks/OpenCL.framework/OpenCL"]
    } else {
        vec!["libOpenCL.so.1", "libOpenCL.so"]
    }
}

/// Enumerate the OpenCL platforms and devices. Err with a message if OpenCL is not available.
pub fn enumerate(extra_lib_dirs: &[std::path::PathBuf]) -> Result<Vec<ClPlatform>, String> {
    let mut lib: Option<Library> = None;
    let mut last_err = String::from("OpenCL library not found");
    let mut candidates: Vec<std::path::PathBuf> = library_names().iter().map(std::path::PathBuf::from).collect();
    for d in extra_lib_dirs {
        for n in library_names() {
            candidates.push(d.join(n));
        }
    }
    for c in candidates {
        match unsafe { Library::new(&c) } {
            Ok(l) => {
                lib = Some(l);
                break;
            }
            Err(e) => last_err = e.to_string(),
        }
    }
    let lib = lib.ok_or(last_err)?;

    unsafe {
        let get_platform_ids: Symbol<FnGetPlatformIds> = lib.get(b"clGetPlatformIDs\0").map_err(|e| e.to_string())?;
        let get_device_ids: Symbol<FnGetDeviceIds> = lib.get(b"clGetDeviceIDs\0").map_err(|e| e.to_string())?;
        let api = Api {
            get_platform_info: lib.get(b"clGetPlatformInfo\0").map_err(|e| e.to_string())?,
            get_device_info: lib.get(b"clGetDeviceInfo\0").map_err(|e| e.to_string())?,
        };

        let mut n: ClUint = 0;
        let err = get_platform_ids(0, std::ptr::null_mut(), &mut n);
        if err != 0 {
            return Err(format!("clGetPlatformIDs failed ({})", err));
        }
        if n == 0 {
            return Ok(Vec::new());
        }
        let mut ids: Vec<ClPlatformId> = vec![std::ptr::null_mut(); n as usize];
        if get_platform_ids(n, ids.as_mut_ptr(), &mut n) != 0 {
            return Err("clGetPlatformIDs failed".into());
        }

        let mut out = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        let mut platform_id = 0usize;
        for p in ids {
            let vendor_raw = api.platform_str(p, CL_PLATFORM_VENDOR);
            let vendor = normalize_vendor(&vendor_raw);
            let pname = format!("{} {}", api.platform_str(p, CL_PLATFORM_NAME), api.platform_str(p, CL_PLATFORM_VERSION));
            if !(cfg!(target_os = "linux") && vendor == "AMD") && seen.iter().any(|s| s.eq_ignore_ascii_case(&pname)) {
                continue;
            }
            let mut dn: ClUint = 0;
            let e = get_device_ids(p, CL_DEVICE_TYPE_ALL, 0, std::ptr::null_mut(), &mut dn);
            if e != 0 || dn == 0 {
                continue;
            }
            let mut dids: Vec<ClDeviceId> = vec![std::ptr::null_mut(); dn as usize];
            if get_device_ids(p, CL_DEVICE_TYPE_ALL, dn, dids.as_mut_ptr(), &mut dn) != 0 {
                continue;
            }
            seen.push(pname.clone());
            let pversion = api.platform_str(p, CL_PLATFORM_VERSION);
            let mut devices = Vec::new();
            for (i, d) in dids.into_iter().enumerate() {
                let dvendor = api.device_str(d, CL_DEVICE_VENDOR);
                let lv = dvendor.to_lowercase();
                let architecture = api.device_str(d, CL_DEVICE_NAME);
                let mut name = architecture.clone();
                if lv.contains("advanced micro devices") {
                    let b = api.device_str(d, CL_DEVICE_BOARD_NAME_AMD);
                    if !b.is_empty() {
                        name = b;
                    }
                }
                let t = api.device_u64(d, CL_DEVICE_TYPE).unwrap_or(0);
                let dtype = if t & 4 != 0 {
                    "Gpu"
                } else if t & 2 != 0 {
                    "Cpu"
                } else if t & 8 != 0 {
                    "Accelerator"
                } else {
                    "Default"
                };
                let mut capability = None;
                let mut pci = None;
                if lv.contains("nvidia") {
                    if let (Some(a), Some(b)) = (
                        api.device_u32(d, CL_DEVICE_COMPUTE_CAPABILITY_MAJOR_NV),
                        api.device_u32(d, CL_DEVICE_COMPUTE_CAPABILITY_MINOR_NV),
                    ) {
                        capability = Some(format!("{}.{}", a, b));
                    }
                    if let (Some(bus), Some(slot)) = (
                        api.device_u32(d, CL_DEVICE_PCI_BUS_ID_NV),
                        api.device_u32(d, CL_DEVICE_PCI_SLOT_ID_NV),
                    ) {
                        pci = Some(format!("{:02X}:{:02X}", bus, slot >> 3));
                    }
                } else {
                    if let Some(b) = api.device_bytes(d, CL_DEVICE_TOPOLOGY_AMD) {
                        if b.len() == 24 && b[0] == 1 {
                            pci = Some(format!("{:02X}:{:02X}", b[21], b[22]));
                        }
                    }
                    if pci.is_none() {
                        if let Some(b) = api.device_bytes(d, CL_DEVICE_PCI_BUS_INFO_KHR) {
                            if b.len() >= 16 {
                                let bus = u32::from_ne_bytes([b[4], b[5], b[6], b[7]]);
                                let dev = u32::from_ne_bytes([b[8], b[9], b[10], b[11]]);
                                pci = Some(format!("{:02X}:{:02X}", bus, dev));
                            }
                        }
                    }
                }
                devices.push(ClDevice {
                    device_index: i,
                    name,
                    architecture,
                    dtype: dtype.to_string(),
                    vendor: dvendor,
                    global_mem_size: api.device_u64(d, CL_DEVICE_GLOBAL_MEM_SIZE).unwrap_or(0),
                    max_mem_alloc_size: api.device_u64(d, CL_DEVICE_MAX_MEM_ALLOC_SIZE).unwrap_or(0),
                    max_compute_units: api.device_u32(d, CL_DEVICE_MAX_COMPUTE_UNITS).unwrap_or(0),
                    platform_version: pversion.clone(),
                    driver_version: api.device_str(d, CL_DRIVER_VERSION),
                    pci_bus_id: pci,
                    device_capability: capability,
                });
            }
            out.push(ClPlatform {
                platform_id,
                vendor,
                name: pname,
                devices,
            });
            platform_id += 1;
        }
        Ok(out)
    }
}
