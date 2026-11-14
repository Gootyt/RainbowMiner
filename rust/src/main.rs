#![recursion_limit = "512"]

//! RainbowMiner (Rust port): a profit switching crypto mining manager.
//! Run it from the RainbowMiner folder (Config/, Data/, Stats/, Bin/, web/).

mod api;
mod balances;
mod combos;
mod config;
mod data;
mod device;
mod display;
mod download;
mod engine;
mod jsonx;
mod log;
mod miner;
mod minerapi;
mod miners;
mod opencl;
mod params;
mod pool;
mod pools;
mod proc;
mod rates;
mod stat;
mod util;
mod util_term;
mod web;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// set by Ctrl+C / SIGTERM
pub static INTERRUPTED: AtomicBool = AtomicBool::new(false);

pub const VERSION: &str = "5.0.4.1";

/// the RainbowMiner folder: -BaseDir, the current folder, or a parent of the executable
fn find_base(cl: &params::CommandLine) -> PathBuf {
    let ok = |p: &Path| p.join("Data").join("algorithms.json").exists();
    if let Some(b) = &cl.base_dir {
        return PathBuf::from(b);
    }
    if let Ok(cwd) = std::env::current_dir() {
        if ok(&cwd) {
            return cwd;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut d = exe.parent().map(|p| p.to_path_buf());
        while let Some(p) = d {
            if ok(&p) {
                return p;
            }
            d = p.parent().map(|x| x.to_path_buf());
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn list_devices(base: &Path) {
    let cpu = device::detect_cpu(base);
    let devs = device::detect_devices(&device::DetectOptions { base: base.to_path_buf(), opencl_platform_sorting: vec![], nvsmi_path: String::new() }, &cpu);
    println!("{:<8} {:<8} {:<28} {:<14} {:>8} {}", "Name", "Vendor", "Model_Name", "Model", "Mem(GB)", "BusId");
    for d in &devs {
        println!(
            "{:<8} {:<8} {:<28} {:<14} {:>8} {}",
            d.Name,
            d.Vendor,
            d.Model_Name,
            d.Model,
            if d.is_gpu() { format!("{:.1}", d.OpenCL.GlobalMemSize as f64 / 1073741824.0) } else { "-".into() },
            d.BusId.clone().unwrap_or_default()
        );
    }
    println!("\nCPU: {} ({} cores / {} threads)", cpu.Name, cpu.Cores, cpu.Threads);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cl = match params::parse_args(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}\n\n{}", e, params::usage());
            std::process::exit(1);
        }
    };
    if cl.help {
        println!("{}", params::usage());
        return;
    }
    if cl.version {
        println!("RainbowMiner {} (Rust port)", VERSION);
        return;
    }
    let base = find_base(&cl);
    if let Err(e) = std::env::set_current_dir(&base) {
        eprintln!("Cannot change to {}: {}", base.display(), e);
    }
    data::init(&base);
    stat::set_stats_dir(base.join("Stats"));
    log::init(base.join("Logs"), log::LogSetting::parse("Info"));
    if cl.list_devices {
        list_devices(&base);
        return;
    }
    if cl.list_pools {
        for p in pools::names() {
            println!("{}", p);
        }
        return;
    }
    if cl.list_miners {
        for m in miners::names() {
            println!("{}", m);
        }
        return;
    }
    if cl.setup {
        println!("The interactive setup is not part of the Rust port. Edit Config/config.txt and Config/pools.config.txt, or use the web interface.");
        return;
    }
    let _ = ctrlc::set_handler(|| {
        if INTERRUPTED.swap(true, Ordering::SeqCst) {
            // second Ctrl+C: leave at once
            util_term::restore();
            std::process::exit(130);
        }
    });
    log_info!("Starting RainbowMiner v{} (Rust port) in {}", VERSION, base.display());
    let mut core = match engine::Core::start(base.clone(), VERSION.to_string(), cl) {
        Ok(c) => c,
        Err(e) => {
            log_error!("Startup failed: {}", e);
            util_term::restore();
            std::process::exit(1);
        }
    };
    let mut restart = false;
    loop {
        if INTERRUPTED.load(Ordering::SeqCst) {
            break;
        }
        if !core.run_round() {
            restart = !core.stopp && !INTERRUPTED.load(Ordering::SeqCst);
            break;
        }
    }
    core.stop();
    log_info!("RainbowMiner stopped.");
    if restart {
        log_info!("Restarting RainbowMiner ..");
        if let Ok(exe) = std::env::current_exe() {
            let args: Vec<String> = std::env::args().skip(1).collect();
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                let err = std::process::Command::new(&exe).args(&args).exec();
                log_error!("Restart failed: {}", err);
            }
            #[cfg(not(unix))]
            {
                let _ = std::process::Command::new(&exe).args(&args).spawn();
            }
        }
    }
}
