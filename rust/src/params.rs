//! The parameters of RainbowMiner.ps1 (name, type, default) and a PowerShell-like
//! command line parser: `rainbowminer -Wallet 1abc -WorkerName rig1 -DeviceName gpu,cpu -Watchdog`

use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PType {
    Str,
    Int,
    Double,
    /// [Switch] and [Bool]
    Bool,
    Array,
}

pub struct ParamDef {
    pub name: &'static str,
    pub ptype: PType,
    pub default: fn() -> Value,
    pub aliases: &'static [&'static str],
}

macro_rules! p {
    ($name:literal, Str, $def:expr) => { ParamDef { name: $name, ptype: PType::Str, default: || json!($def), aliases: &[] } };
    ($name:literal, Str, $def:expr, [$($al:literal),*]) => { ParamDef { name: $name, ptype: PType::Str, default: || json!($def), aliases: &[$($al),*] } };
    ($name:literal, Int, $def:expr) => { ParamDef { name: $name, ptype: PType::Int, default: || json!($def), aliases: &[] } };
    ($name:literal, Double, $def:expr) => { ParamDef { name: $name, ptype: PType::Double, default: || json!($def as f64), aliases: &[] } };
    ($name:literal, Bool, $def:expr) => { ParamDef { name: $name, ptype: PType::Bool, default: || json!($def), aliases: &[] } };
    ($name:literal, Array, [$($v:literal),*]) => { ParamDef { name: $name, ptype: PType::Array, default: || json!([$($v),*]), aliases: &[] } };
    ($name:literal, Array, [$($v:literal),*], [$($al:literal),*]) => { ParamDef { name: $name, ptype: PType::Array, default: || json!([$($v),*]), aliases: &[$($al),*] } };
    ($name:literal, Fn, $t:ident, $f:expr) => { ParamDef { name: $name, ptype: PType::$t, default: $f, aliases: &[] } };
}

fn default_msiapath() -> Value {
    let pf = std::env::var("ProgramFiles(x86)").unwrap_or_else(|_| "C:\\Program Files (x86)".into());
    json!(format!("{}\\MSI Afterburner\\MSIAfterburner.exe", pf))
}

fn default_nvsmipath() -> Value {
    let pf = std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".into());
    json!(format!("{}\\NVIDIA Corporation\\NVSMI", pf))
}

pub static PARAMS: &[ParamDef] = &[
    p!("Wallet", Str, "", ["BTC"]),
    p!("WorkerName", Str, "", ["Worker"]),
    p!("Interval", Int, 60),
    p!("Region", Str, "us", ["Location"]),
    p!("DefaultPoolRegion", Array, ["us", "eu"]),
    p!("SSL", Int, 0),
    p!("DeviceName", Array, [], ["Device", "Type"]),
    p!("ExcludeDeviceName", Array, [], ["ExcludeDevice"]),
    p!("Algorithm", Array, []),
    p!("MinerName", Array, [], ["Miner"]),
    p!("PoolName", Array, [], ["Pool"]),
    p!("ExcludeAlgorithm", Array, []),
    p!("ExcludeMinerName", Array, [], ["ExcludeMiner"]),
    p!("PreferMinerName", Array, [], ["PreferMiner"]),
    p!("PreferMinerMargin", Double, 5),
    p!("ExcludePoolName", Array, [], ["ExcludePool"]),
    p!("ExcludeCoin", Array, []),
    p!("ExcludeCoinSymbol", Array, []),
    p!("OpenCLPlatformSorting", Array, [], ["PlatformSorting"]),
    p!("Currency", Array, ["BTC", "USD"]),
    p!("ExcludeCurrencyRateFromCoinbase", Array, ["BIT", "PRL"]),
    p!("Donate", Int, 24),
    p!("Proxy", Str, ""),
    p!("ProxyUsername", Str, ""),
    p!("ProxyPassword", Str, ""),
    p!("WebsitesForOnlineCheck", Array, ["www.google.com", "www.amazon.com", "www.coinbase.com", "www.sina.com"]),
    p!("Delay", Int, 1),
    p!("Watchdog", Bool, false),
    p!("ExcludeFromWatchdog", Array, []),
    p!("MinerStatusUrl", Str, "https://api.rbminer.net", ["Uri", "Url"]),
    p!("MinerStatusKey", Str, ""),
    p!("ShowRemoteMachines", Str, ""),
    p!("MinerStatusEmail", Str, ""),
    p!("MinerStatusMaxTemp", Double, 90.0),
    p!("DiskMinGB", Double, 5.0),
    p!("MinFreeMemoryGB", Double, 1.5),
    p!("GPUReservedVRAMGB", Str, ""),
    p!("MinerStatusMaxCrashesPerHour", Int, 5),
    p!("PushOverUserKey", Str, ""),
    p!("SwitchingPrevention", Double, 2),
    p!("PoolSwitchingHysteresis", Double, 3),
    p!("MinerSwitchingHysteresis", Double, 3),
    p!("MaxRejectedShareRatio", Double, 0.3),
    p!("MaxAllowedLuck", Double, 3),
    p!("MaxTimeSinceLastBlock", Str, "12h"),
    p!("MinComboOverSingleRatio", Double, 0.9),
    p!("ShowMinerWindow", Bool, false),
    p!("FastestMinerOnly", Bool, false),
    p!("IgnoreFees", Bool, false),
    p!("ExcludeMinersWithFee", Bool, false),
    p!("DisableUnprofitableAlgolist", Bool, false),
    p!("DisableUnprofitableCpuAlgolist", Bool, false),
    p!("EnableNeverprofitableAlgos", Bool, false),
    p!("EnableCheckMiningConflict", Bool, false),
    p!("EnableCurl", Bool, false),
    p!("EnableEthashZombieMode", Bool, false),
    p!("ShowPoolBalances", Bool, false),
    p!("ShowPoolBalancesDetails", Bool, false),
    p!("ShowPoolBalancesExcludedPools", Bool, false),
    p!("ExcludeCoinsymbolBalances", Array, []),
    p!("ShowWalletBalances", Bool, false),
    p!("WalletBalances", Array, []),
    p!("DisableDualMining", Bool, false),
    p!("EnableDualMiningDuringRentals", Bool, false),
    p!("DisableZeroWattMiners", Bool, false),
    p!("EnableRandomX1GBPages", Bool, false),
    p!("APIport", Int, 4000),
    p!("APIuser", Str, ""),
    p!("APIpassword", Str, ""),
    p!("APIauth", Bool, false),
    p!("APIlockConfig", Bool, false),
    p!("APIthreads", Int, 0),
    p!("APImaxLoginAttemps", Int, 3),
    p!("APIblockLoginAttemptsTime", Str, "30m"),
    p!("APIallowIPs", Array, []),
    p!("RebootOnGPUFailure", Bool, false),
    p!("MiningMode", Str, "combo"),
    p!("MSIApath", Fn, Str, default_msiapath),
    p!("MSIAprofile", Int, 0),
    p!("UIstyle", Str, "lite"),
    p!("UIsorting", Str, "biased"),
    p!("UIFullBenchmarkList", Bool, false),
    p!("UIProfitColumns", Array, ["Miner", "Fee", "Algorithm", "Speed", "Diff", "Power", "Profit", "TTF", "Accuracy", "Pool", "PoolFee", "Wallet"]),
    p!("UseTimeSync", Bool, false),
    p!("PowerPrice", Double, 0),
    p!("PowerPriceCurrency", Str, "USD"),
    p!("UsePowerPrice", Bool, false),
    p!("PowerOffset", Double, 0),
    p!("PowerOffsetPercent", Double, 0),
    p!("PowerCPUtdp", Double, 0),
    p!("OctopusTariffCode", Str, ""),
    p!("PowerPriceApi", Str, ""),
    p!("PowerPriceApiValue", Str, ""),
    p!("PowerPriceApiInterval", Int, 0),
    p!("FixedCostPerDay", Double, 0),
    p!("ProfitabilityLevel", Double, 0),
    p!("CheckProfitability", Bool, false),
    p!("DisableExtendInterval", Bool, false),
    p!("EthPillEnable", Str, "disable"),
    p!("EthPillEnableMTP", Str, "disable"),
    p!("EnableOCProfiles", Bool, false),
    p!("EnableOCVoltage", Bool, false),
    p!("EnableOCLinuxSetAllPStates", Bool, false),
    p!("LinuxOCMethod", Str, "nvidia-settings"),
    p!("EnableOCLinuxAmd", Bool, false),
    p!("EnableOCWindowsAmd", Bool, false),
    p!("EnableOCLinuxForcePState", Bool, false),
    p!("OCResetInterval", Int, 0),
    p!("EnableOCFullReset", Bool, false),
    p!("EnableLinuxMinerNiceness", Bool, false),
    p!("LinuxMinerNiceness", Int, 19),
    p!("EnableLinuxHeadless", Bool, false),
    p!("EnableAutoUpdate", Bool, false),
    p!("EnableAutoBenchmark", Bool, false),
    p!("AutoBenchmarkMode", Str, "updated"),
    p!("EnableAutoMinerPorts", Bool, false),
    p!("StaticCPUMinerPort", Int, 0),
    p!("StaticGPUMinerPort", Int, 0),
    p!("DisableMSIAmonitor", Bool, false),
    p!("EnableAutoAdjustAffinity", Bool, false),
    p!("CPUMiningThreads", Int, 0),
    p!("CPUMiningAffinity", Str, ""),
    p!("GPUMiningAffinity", Str, ""),
    p!("DisableAPI", Bool, false),
    p!("DisableAsyncLoader", Bool, false),
    p!("DisableInternetCheck", Bool, false),
    p!("EnableMinerStatus", Bool, true),
    p!("EnableFastSwitching", Bool, false),
    p!("ForceStablePrice", Bool, false),
    p!("EnableMinersAsRoot", Bool, false),
    p!("NVSMIpath", Fn, Str, default_nvsmipath),
    p!("MiningPriorityCPU", Int, -2),
    p!("MiningPriorityGPU", Int, -1),
    p!("AutoexecPriority", Int, -1),
    p!("HashrateWeight", Int, 20),
    p!("HashrateWeightStrength", Int, 50),
    p!("PoolAccuracyWeight", Int, 15),
    p!("EnablePoolAlternates", Bool, true),
    p!("MaxPoolAlternates", Int, 2),
    p!("EnablePoolLatencyPreference", Bool, false),
    p!("MinerFaultToleranceGPU", Int, 10),
    p!("MinerFaultToleranceCPU", Int, 25),
    p!("BalanceUpdateMinutes", Int, 30),
    p!("ProfitSpikeProtection", Double, 0),
    p!("Quickstart", Bool, false),
    p!("PoolDataWindow", Str, ""),
    p!("PoolStatAverage", Str, ""),
    p!("PoolStatAverageStable", Str, ""),
    p!("EnableErrorRatio", Bool, false),
    p!("MaxErrorRatio", Double, 1.5),
    p!("EnableAutoAlgorithmAdd", Bool, false),
    p!("EnableAlgorithmVariants", Bool, false),
    p!("EnableAlgorithmMapping", Bool, false),
    p!("EnableResetVega", Bool, false),
    p!("EnableMiningHeatControl", Bool, false),
    p!("MiningHeatControl", Double, 2),
    p!("MaxActivityDays", Int, 2),
    p!("MaxLogfileDays", Int, 5),
    p!("MaxDownloadfileDays", Int, 14),
    p!("MaxCachefileDays", Int, 14),
    p!("StartPaused", Bool, false),
    p!("EnablePauseOnActivity", Bool, false),
    p!("EnablePauseOnBattery", Bool, false),
    p!("EnableUpdateDuringPause", Bool, false),
    p!("EnableUpdateWhenScheduled", Bool, false),
    p!("ResumeOnInactivitySeconds", Int, 300),
    p!("EnableFastlaneBenchmark", Bool, false),
    p!("FastlaneBenchmarkTypeCPU", Str, "avg"),
    p!("FastlaneBenchmarkTypeGPU", Str, "avg"),
    p!("EnableFastlaneBenchmarkMissing", Bool, false),
    p!("MinimumMiningIntervals", Int, 1),
    p!("BenchmarkInterval", Int, 60),
    p!("MaxCrashesDuringBenchmark", Int, 2),
    p!("EnableAutoDisableMiners", Bool, false),
    p!("MaxCrashesBeforeDisable", Int, 5),
    p!("CrashTrackingWindowMinutes", Int, 60),
    p!("AutoDisableResetHours", Int, 24),
    p!("EnableStallDetection", Bool, true),
    p!("ServerName", Str, ""),
    p!("ServerPort", Int, 4000),
    p!("ServerSSL", Bool, false),
    p!("ServerCertHash", Str, ""),
    p!("ServerUser", Str, ""),
    p!("ServerPassword", Str, ""),
    p!("EnableServerConfig", Bool, false),
    p!("EnableServerPools", Bool, false),
    p!("EnableServerDiscovery", Bool, false),
    p!("ServerConfigName", Array, []),
    p!("GroupName", Str, ""),
    p!("ExcludeServerConfigVars", Array, ["APIauth", "APIpassword", "APIport", "APIuser", "APIthreads", "CPUMiningAffinity", "CPUMiningThreads", "PowerOffset", "PowerOffsetPercent", "PowerCPUtdp", "DeviceName", "EnableServerConfig", "ExcludeDeviceName", "ExcludeServerConfigVars", "GPUMiningAffinity", "GroupName", "LinuxDisplay", "LinuxXAuthority", "MSIApath", "NVSMIpath", "Proxy", "ProxyUsername", "ProxyPassword", "RunMode", "ServerConfigName", "ServerName", "ServerPassword", "ServerPort", "ServerSSL", "ServerCertHash", "ServerUser", "StartPaused", "WorkerName", "StaticCPUMinerPort", "StaticGPUMinerPort", "OpenCLPlatformSorting"]),
    p!("EnableServerExcludeList", Bool, false),
    p!("EnableMinerBackups", Bool, false),
    p!("EnableKeepDownloads", Bool, false),
    p!("RunMode", Str, "standalone"),
    p!("SetupOnly", Bool, false),
    p!("EnableMinersToSaveDAG", Bool, false),
    p!("EnableDebugMode", Bool, false),
    p!("EnableDebugTimers", Bool, false),
    p!("EnableVerboseAsyncloader", Bool, false),
    p!("EnableRestartComputer", Bool, false),
    p!("RestartComputerHours", Double, 0),
    p!("RestartRBMTimespan", Str, ""),
    p!("RestartRBMMemory", Str, "16GB"),
    p!("LinuxDisplay", Str, ":0"),
    p!("LinuxXAuthority", Str, ""),
    p!("LinuxMinerTerminal", Str, "auto"),
    p!("CovalentAPIKey", Str, ""),
    p!("UserAgent", Str, ""),
    p!("LogLevel", Str, "Info"),
];

pub fn find(name: &str) -> Option<&'static ParamDef> {
    PARAMS.iter().find(|p| {
        p.name.eq_ignore_ascii_case(name) || p.aliases.iter().any(|a| a.eq_ignore_ascii_case(name))
    })
}

/// Parsed command line
#[derive(Debug, Default, Clone)]
pub struct CommandLine {
    /// parameter name (canonical) -> value
    pub values: indexmap::IndexMap<String, Value>,
    pub config_file: Option<String>,
    pub base_dir: Option<String>,
    pub help: bool,
    pub version: bool,
    pub list_devices: bool,
    pub list_pools: bool,
    pub list_miners: bool,
    pub setup: bool,
}

fn to_array(s: &str) -> Value {
    Value::Array(
        crate::util::split_config_array(s)
            .into_iter()
            .map(Value::String)
            .collect(),
    )
}

/// Parse `-Name value`, `-Name:value`, `--name value`, `-Switch`, `-Switch:$false`
pub fn parse_args(args: &[String]) -> Result<CommandLine, String> {
    let mut cl = CommandLine::default();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        let stripped = a.trim_start_matches('-');
        if stripped.len() == a.len() {
            return Err(format!("unexpected argument '{}'", a));
        }
        let (name, inline_val) = match stripped.split_once([':', '=']) {
            Some((n, v)) => (n.to_string(), Some(v.to_string())),
            None => (stripped.to_string(), None),
        };
        match name.to_lowercase().as_str() {
            "h" | "help" | "?" => {
                cl.help = true;
                i += 1;
                continue;
            }
            "v" | "version" => {
                cl.version = true;
                i += 1;
                continue;
            }
            "listdevices" => {
                cl.list_devices = true;
                i += 1;
                continue;
            }
            "listpools" => {
                cl.list_pools = true;
                i += 1;
                continue;
            }
            "listminers" => {
                cl.list_miners = true;
                i += 1;
                continue;
            }
            "setup" => {
                cl.setup = true;
                i += 1;
                continue;
            }
            "configfile" | "basedir" => {
                let v = match inline_val {
                    Some(v) => v,
                    None => {
                        i += 1;
                        args.get(i).cloned().ok_or_else(|| format!("missing value for -{}", name))?
                    }
                };
                if name.eq_ignore_ascii_case("configfile") {
                    cl.config_file = Some(v);
                } else {
                    cl.base_dir = Some(v);
                }
                i += 1;
                continue;
            }
            _ => {}
        }
        let def = find(&name).ok_or_else(|| format!("unknown parameter -{}", name))?;
        let value = match def.ptype {
            PType::Bool => {
                match inline_val {
                    Some(v) => json!(crate::util::get_yes_str(v.trim_start_matches('$'))),
                    None => {
                        // a following "$true"/"$false"/"1"/"0" is consumed as value
                        if let Some(next) = args.get(i + 1) {
                            let n = next.to_lowercase();
                            if ["$true", "$false", "true", "false", "1", "0"].contains(&n.as_str()) {
                                i += 1;
                                json!(crate::util::get_yes_str(n.trim_start_matches('$')))
                            } else {
                                json!(true)
                            }
                        } else {
                            json!(true)
                        }
                    }
                }
            }
            _ => {
                let v = match inline_val {
                    Some(v) => v,
                    None => {
                        i += 1;
                        args.get(i).cloned().ok_or_else(|| format!("missing value for -{}", def.name))?
                    }
                };
                match def.ptype {
                    PType::Int => json!(v.trim().parse::<f64>().map_err(|_| format!("-{} expects a number", def.name))?.round() as i64),
                    PType::Double => json!(v.trim().replace(',', ".").parse::<f64>().map_err(|_| format!("-{} expects a number", def.name))?),
                    PType::Array => to_array(&v),
                    _ => json!(v),
                }
            }
        };
        cl.values.insert(def.name.to_string(), value);
        i += 1;
    }
    Ok(cl)
}

/// Default values: parameter defaults overlaid with the command line ($Session.DefaultValues)
pub fn default_values(cl: &CommandLine) -> indexmap::IndexMap<String, Value> {
    let mut m = indexmap::IndexMap::new();
    for p in PARAMS {
        let v = cl.values.get(p.name).cloned().unwrap_or_else(|| (p.default)());
        m.insert(p.name.to_string(), v);
    }
    m
}

pub fn ptype_of(name: &str) -> Option<PType> {
    PARAMS.iter().find(|p| p.name.eq_ignore_ascii_case(name)).map(|p| p.ptype)
}

pub fn usage() -> String {
    let mut s = String::from(
        "RainbowMiner (Rust port)\n\nUsage: rainbowminer [-Parameter value] [-Switch] ...\n\n\
         Special options:\n  -ConfigFile <path>   config file (default Config/config.txt)\n  \
         -BaseDir <path>      RainbowMiner folder (Data, Config, Stats, Bin, web)\n  \
         -Setup               (the interactive setup is not part of the Rust port)\n  -ListDevices         show the detected devices and exit\n  \
         -ListPools           show the available pool modules and exit\n  -ListMiners          show the available miner modules and exit\n  \
         -Version             print the version\n\nParameters (same names as in RainbowMiner.ps1 / config.txt):\n",
    );
    for p in PARAMS {
        let t = match p.ptype {
            PType::Str => "string",
            PType::Int => "int",
            PType::Double => "double",
            PType::Bool => "switch",
            PType::Array => "list",
        };
        s.push_str(&format!("  -{:<34} {:<7} default: {}\n", p.name, t, (p.default)()));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli() {
        let args: Vec<String> = ["-Wallet", "1abc", "-worker", "rig1", "-DeviceName", "gpu,cpu", "-Watchdog", "-Interval:90"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let cl = parse_args(&args).unwrap();
        assert_eq!(cl.values["Wallet"], json!("1abc"));
        assert_eq!(cl.values["WorkerName"], json!("rig1"));
        assert_eq!(cl.values["DeviceName"], json!(["gpu", "cpu"]));
        assert_eq!(cl.values["Watchdog"], json!(true));
        assert_eq!(cl.values["Interval"], json!(90));
        assert!(parse_args(&["-Nope".to_string()]).is_err());
    }
}
