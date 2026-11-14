//! Process management for the miners (Modules/ProcLib.psm1):
//! start in an own process group, stdout/stderr into the miner log file (and to the wrapper parser),
//! priority / niceness, CPU affinity, graceful stop, rogue process detection.
#![allow(dead_code)]

use crate::{log_info, log_warn};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct StartOptions {
    pub file_path: PathBuf,
    pub arguments: String,
    pub log_path: Option<PathBuf>,
    pub working_dir: PathBuf,
    /// -2 (idle) .. 3 (realtime), like the PowerShell version
    pub priority: i32,
    pub cpu_affinity: Vec<usize>,
    pub env_vars: Vec<String>,
    pub vendor: String,
    pub set_ld_library_path: bool,
    pub ld_library_dir: Option<PathBuf>,
    pub niceness: Option<i32>,
    /// keep stdout lines for the wrapper parsers
    pub capture: bool,
}

pub struct MinerProcess {
    child: Child,
    pub pid: u32,
    pub start_time: DateTime<Utc>,
    pub exit_time: Option<DateTime<Utc>>,
    rx: Option<Receiver<String>>,
    pub log_path: Option<PathBuf>,
    exited: bool,
}

fn vendor_env(vendor: &str) -> Vec<(String, String)> {
    let gpu = vec![
        ("GPU_MAX_HEAP_SIZE", "100"),
        ("GPU_MAX_USE_SYNC_OBJECTS", "1"),
        ("GPU_SINGLE_ALLOC_PERCENT", "100"),
        ("GPU_MAX_ALLOC_PERCENT", "100"),
        ("GPU_MAX_SINGLE_ALLOC_PERCENT", "100"),
        ("GPU_ENABLE_LARGE_ALLOCATION", "100"),
        ("GPU_MAX_WORKGROUP_SIZE", "256"),
    ];
    let v: Vec<(&str, &str)> = match vendor {
        "AMD" | "INTEL" => gpu,
        "NVIDIA" => vec![("CUDA_DEVICE_ORDER", "PCI_BUS_ID")],
        _ => vec![],
    };
    v.into_iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

#[cfg(windows)]
fn priority_flags(p: i32) -> u32 {
    match p {
        i32::MIN..=-2 => 0x0000_0040, // IDLE_PRIORITY_CLASS
        -1 => 0x0000_4000,            // BELOW_NORMAL_PRIORITY_CLASS
        0 => 0x0000_0020,             // NORMAL_PRIORITY_CLASS
        1 => 0x0000_8000,             // ABOVE_NORMAL_PRIORITY_CLASS
        2 => 0x0000_0080,             // HIGH_PRIORITY_CLASS
        _ => 0x0000_0100,             // REALTIME_PRIORITY_CLASS
    }
}

impl MinerProcess {
    pub fn start(o: &StartOptions) -> Result<MinerProcess> {
        if !o.file_path.exists() {
            return Err(anyhow!("{} not found", o.file_path.display()));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(m) = std::fs::metadata(&o.file_path) {
                let mut p = m.permissions();
                if p.mode() & 0o111 == 0 {
                    p.set_mode(p.mode() | 0o755);
                    let _ = std::fs::set_permissions(&o.file_path, p);
                }
            }
        }

        let mut cmd = Command::new(&o.file_path);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // keep the argument string verbatim, like Start-Process does
            if !o.arguments.trim().is_empty() {
                cmd.raw_arg(o.arguments.trim());
            }
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW | priority_flags(o.priority));
        }
        #[cfg(not(windows))]
        {
            cmd.args(crate::util::split_args(&o.arguments));
        }
        cmd.current_dir(&o.working_dir);
        let mut env = vendor_env(&o.vendor);
        let mut extra_ld = None;
        for e in &o.env_vars {
            if let Some((k, v)) = e.split_once('=') {
                let k = k.trim().to_string();
                let v = v.trim().to_string();
                if k == "LD_LIBRARY_PATH" {
                    extra_ld = Some(v);
                    continue;
                }
                env.retain(|(a, _)| *a != k);
                env.push((k, v));
            }
        }
        for (k, v) in env {
            cmd.env(k, v);
        }
        if cfg!(unix) {
            if o.set_ld_library_path {
                let mut p = String::from("./");
                if let Some(d) = &o.ld_library_dir {
                    p.push(':');
                    p.push_str(&d.display().to_string());
                }
                if let Some(x) = &extra_ld {
                    p.push(':');
                    p.push_str(x);
                }
                if let Ok(old) = std::env::var("LD_LIBRARY_PATH") {
                    if !old.is_empty() {
                        p.push(':');
                        p.push_str(&old);
                    }
                }
                cmd.env("LD_LIBRARY_PATH", p);
            } else if let Some(x) = &extra_ld {
                cmd.env("LD_LIBRARY_PATH", x);
            }
        }
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
            let aff = o.cpu_affinity.clone();
            let nice = o.niceness;
            unsafe {
                cmd.pre_exec(move || {
                    if let Some(n) = nice {
                        libc::setpriority(libc::PRIO_PROCESS, 0, n);
                    }
                    #[cfg(target_os = "linux")]
                    if !aff.is_empty() {
                        let mut set: libc::cpu_set_t = std::mem::zeroed();
                        for c in &aff {
                            if *c < libc::CPU_SETSIZE as usize {
                                libc::CPU_SET(*c, &mut set);
                            }
                        }
                        libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
                    }
                    #[cfg(not(target_os = "linux"))]
                    let _ = &aff;
                    Ok(())
                });
            }
        }

        let mut child = cmd.spawn().map_err(|e| anyhow!("cannot start {}: {}", o.file_path.display(), e))?;
        let pid = child.id();

        #[cfg(windows)]
        if !o.cpu_affinity.is_empty() {
            log_info!("CPU affinity is not applied on Windows by the Rust port (pid {})", pid);
        }

        let log_file: Option<Arc<Mutex<std::fs::File>>> = o.log_path.as_ref().and_then(|p| {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            std::fs::OpenOptions::new().create(true).append(true).open(p).ok().map(|f| Arc::new(Mutex::new(f)))
        });
        let (tx, rx) = channel::<String>();
        let capture = o.capture;
        let out = child.stdout.take();
        let err = child.stderr.take();
        for stream in [out.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>), err.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>)]
            .into_iter()
            .flatten()
        {
            let lf = log_file.clone();
            let tx = tx.clone();
            std::thread::spawn(move || {
                let mut rd = BufReader::new(stream);
                let mut buf = Vec::new();
                loop {
                    buf.clear();
                    match rd.read_until(b'\n', &mut buf) {
                        Ok(0) => break,
                        Ok(_) => {
                            if let Some(f) = &lf {
                                if let Ok(mut f) = f.lock() {
                                    let _ = f.write_all(&buf);
                                }
                            }
                            if capture {
                                let line = String::from_utf8_lossy(&buf);
                                // progress lines with \r: keep the last segment
                                for part in line.split('\r') {
                                    let p = part.trim_end_matches('\n');
                                    if !p.trim().is_empty() {
                                        let _ = tx.send(p.to_string());
                                    }
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }
        Ok(MinerProcess {
            child,
            pid,
            start_time: Utc::now(),
            exit_time: None,
            rx: Some(rx),
            log_path: o.log_path.clone(),
            exited: false,
        })
    }

    pub fn is_running(&mut self) -> bool {
        if self.exited {
            return false;
        }
        match self.child.try_wait() {
            Ok(None) => true,
            Ok(Some(_)) | Err(_) => {
                self.exited = true;
                self.exit_time = Some(Utc::now());
                false
            }
        }
    }

    /// all output lines since the last call (Read-MinerJobOutput)
    pub fn drain_output(&self) -> Vec<String> {
        match &self.rx {
            Some(rx) => rx.try_iter().collect(),
            None => Vec::new(),
        }
    }

    pub fn has_output(&self) -> bool {
        true
    }

    /// Stop-SubProcess: graceful stop (Ctrl+C / SIGINT to the process group), then kill
    pub fn stop(&mut self, title: &str, shutdown_url: &str, skip_wait: bool) {
        if !self.is_running() {
            return;
        }
        if !shutdown_url.is_empty() {
            let _ = crate::web::get_text(shutdown_url, 5);
            if self.wait_exit(Duration::from_secs(10)) {
                log_info!("{} closed gracefully via shutdown url", title);
                return;
            }
        }
        #[cfg(unix)]
        unsafe {
            let pgid = self.pid as i32;
            libc::kill(-pgid, libc::SIGINT);
            if !skip_wait && self.wait_exit(Duration::from_secs(20)) {
                log_info!("{} closed gracefully", title);
                return;
            }
            libc::kill(-pgid, libc::SIGTERM);
            if self.wait_exit(Duration::from_secs(5)) {
                log_info!("{} terminated", title);
                return;
            }
            libc::kill(-pgid, libc::SIGKILL);
        }
        #[cfg(not(unix))]
        {
            let _ = skip_wait;
            let _ = Command::new("taskkill").args(["/PID", &self.pid.to_string(), "/T"]).output();
            if self.wait_exit(Duration::from_secs(10)) {
                log_info!("{} closed gracefully", title);
                return;
            }
            let _ = Command::new("taskkill").args(["/PID", &self.pid.to_string(), "/T", "/F"]).output();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.exited = true;
        self.exit_time = Some(Utc::now());
        log_warn!("{} had to be killed", title);
    }

    fn wait_exit(&mut self, t: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < t {
            if !self.is_running() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        false
    }
}

impl Drop for MinerProcess {
    fn drop(&mut self) {
        if self.is_running() {
            self.stop("Miner", "", true);
        }
    }
}

/// run a helper command and wait (Invoke-Exe)
pub fn invoke_exe(file: &Path, args: &[String], working_dir: Option<&Path>, timeout: Duration) -> Result<String> {
    let mut cmd = Command::new(file);
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());
    if let Some(w) = working_dir {
        cmd.current_dir(w);
    }
    let mut child = cmd.spawn()?;
    let start = Instant::now();
    loop {
        if let Some(_st) = child.try_wait()? {
            let out = child.wait_with_output()?;
            let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
            s.push_str(&String::from_utf8_lossy(&out.stderr));
            return Ok(s);
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            return Err(anyhow!("timeout"));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// find miner processes started from the Bin folder that are not ours (Kill marauding miners)
pub fn find_rogue_processes(bin_dir: &Path, tracked: &HashSet<u32>, exe_names: &HashSet<String>) -> Vec<(u32, String)> {
    use sysinfo::{ProcessRefreshKind, RefreshKind, System, UpdateKind};
    let sys = System::new_with_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::new().with_exe(UpdateKind::Always)));
    let bin = std::fs::canonicalize(bin_dir).unwrap_or_else(|_| bin_dir.to_path_buf());
    let mut out = Vec::new();
    // the descendants of our processes are shielded
    let mut shielded: HashSet<u32> = tracked.clone();
    loop {
        let mut added = false;
        for (pid, p) in sys.processes() {
            let pid = pid.as_u32();
            if shielded.contains(&pid) {
                continue;
            }
            if let Some(pp) = p.parent() {
                if shielded.contains(&pp.as_u32()) {
                    shielded.insert(pid);
                    added = true;
                }
            }
        }
        if !added {
            break;
        }
    }
    for (pid, p) in sys.processes() {
        let pid = pid.as_u32();
        if shielded.contains(&pid) || pid == std::process::id() {
            continue;
        }
        let Some(exe) = p.exe() else { continue };
        if !exe.starts_with(&bin) {
            continue;
        }
        let name = exe.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
        if exe_names.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
            out.push((pid, name));
        }
    }
    out
}

pub fn kill_pid(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
        std::thread::sleep(Duration::from_millis(500));
        libc::kill(pid as i32, libc::SIGKILL);
    }
    #[cfg(not(unix))]
    {
        let _ = Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).output();
    }
}

/// list of TCP ports in use (EnableAutoMinerPorts)
pub fn ports_in_use() -> HashSet<u16> {
    let mut out = HashSet::new();
    #[cfg(target_os = "linux")]
    for f in ["/proc/net/tcp", "/proc/net/tcp6"] {
        if let Ok(s) = std::fs::read_to_string(f) {
            for line in s.lines().skip(1) {
                let cols: Vec<&str> = line.split_whitespace().collect();
                if cols.len() > 2 {
                    if let Some(port) = cols[1].rsplit(':').next() {
                        if let Ok(p) = u16::from_str_radix(port, 16) {
                            out.insert(p);
                        }
                    }
                }
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    if let Ok(o) = Command::new("netstat").arg("-an").output() {
        let s = String::from_utf8_lossy(&o.stdout);
        for line in s.lines() {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() > 1 && cols[0].starts_with("TCP") {
                if let Some(p) = cols[1].rsplit(':').next().and_then(|x| x.parse::<u16>().ok()) {
                    out.insert(p);
                }
            }
        }
    }
    out
}
