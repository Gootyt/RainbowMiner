//! English and Hungarian texts of the GUI

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Hu,
}

impl Lang {
    /// Hungarian when the system locale is Hungarian
    pub fn detect() -> Lang {
        for k in ["RBM_LANG", "LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(v) = std::env::var(k) {
                if !v.is_empty() {
                    return if v.to_lowercase().starts_with("hu") { Lang::Hu } else { Lang::En };
                }
            }
        }
        Lang::En
    }
}

/// (key, english, hungarian)
const TEXTS: &[(&str, &str, &str)] = &[
    ("dashboard", "Dashboard", "Áttekintés"),
    ("devices", "Devices", "Eszközök"),
    ("miners", "Miners", "Bányászok"),
    ("pools", "Pools", "Poolok"),
    ("balances", "Balances", "Egyenlegek"),
    ("benchmarks", "Benchmarks", "Benchmarkok"),
    ("console", "Console", "Konzol"),
    ("settings", "Settings", "Beállítások"),
    ("connected", "connected", "kapcsolódva"),
    ("disconnected", "not connected", "nincs kapcsolat"),
    ("running", "running", "fut"),
    ("paused", "paused", "szüneteltetve"),
    ("locked", "miners locked", "bányászok rögzítve"),
    ("exclusive", "exclusive run", "exkluzív futás"),
    ("pause", "Pause", "Szünet"),
    ("resume", "Resume", "Folytatás"),
    ("lock", "Lock miners", "Bányászok rögzítése"),
    ("unlock", "Unlock miners", "Rögzítés feloldása"),
    ("watchdogreset", "Reset watchdog", "Watchdog nullázása"),
    ("updatebalance", "Update balances", "Egyenlegek frissítése"),
    ("clearcache", "Clear cache", "Gyorsítótár törlése"),
    ("stop", "Stop engine", "Engine leállítása"),
    ("start", "Start engine", "Engine indítása"),
    ("refresh", "Refresh", "Frissítés"),
    ("confirm_stop", "Stop RainbowMiner and all miners?", "Leállítod a RainbowMinert és az összes bányászprogramot?"),
    ("yes", "Yes", "Igen"),
    ("no", "No", "Nem"),
    ("profit", "Profit", "Profit"),
    ("profit_day", "Profit / day", "Profit / nap"),
    ("earnings", "Earnings / day", "Bevétel / nap"),
    ("power", "Power", "Fogyasztás"),
    ("uptime", "Uptime", "Futásidő"),
    ("version", "Version", "Verzió"),
    ("name", "Name", "Név"),
    ("model", "Model", "Modell"),
    ("type", "Type", "Típus"),
    ("vendor", "Vendor", "Gyártó"),
    ("temp", "Temp", "Hőm."),
    ("fan", "Fan", "Ventilátor"),
    ("load", "Load", "Terhelés"),
    ("clock", "Clock", "Órajel"),
    ("memory", "Memory", "Memória"),
    ("device", "Device", "Eszköz"),
    ("algorithm", "Algorithm", "Algoritmus"),
    ("coin", "Coin", "Coin"),
    ("pool", "Pool", "Pool"),
    ("speed", "Speed", "Sebesség"),
    ("speed_live", "Live speed", "Élő sebesség"),
    ("status", "Status", "Állapot"),
    ("runtime", "Running", "Fut"),
    ("shares", "Shares A/R", "Share-ek E/V"),
    ("price", "Price", "Ár"),
    ("host", "Host", "Host"),
    ("region", "Region", "Régió"),
    ("hashrate", "Pool hashrate", "Pool hashrate"),
    ("tsl", "Last block", "Utolsó blokk"),
    ("fee", "Fee", "Díj"),
    ("currency", "Currency", "Pénznem"),
    ("total", "Total", "Összesen"),
    ("value", "Value", "Érték"),
    ("running_miners", "Running miners", "Futó bányászok"),
    ("no_miners", "No miner is running.", "Nem fut bányászprogram."),
    ("no_data", "No data yet.", "Még nincs adat."),
    ("filter", "Filter", "Szűrő"),
    ("only_best", "best only", "csak a legjobbak"),
    ("needs_benchmark", "Miners that need a benchmark", "Benchmarkra váró bányászok"),
    ("no_benchmarks", "Every miner is benchmarked.", "Minden bányász benchmarkolva van."),
    ("engine", "Engine", "Engine"),
    ("api_host", "API host", "API host"),
    ("api_port", "API port", "API port"),
    ("api_user", "API user", "API felhasználó"),
    ("api_password", "API password", "API jelszó"),
    ("apply", "Apply", "Alkalmaz"),
    ("interval", "Refresh interval (s)", "Frissítési időköz (mp)"),
    ("language", "Language", "Nyelv"),
    ("base_dir", "RainbowMiner folder", "RainbowMiner mappa"),
    ("engine_exe", "Engine program", "Engine program"),
    ("not_found", "not found", "nem található"),
    ("start_hint", "The engine is not reachable. Start it here or check the API settings.", "Az engine nem érhető el. Indítsd el innen, vagy ellenőrizd az API beállításokat."),
    ("started", "Engine started, waiting for the API ...", "Engine elindítva, várakozás az API-ra ..."),
    ("watchdog", "Watchdog timers", "Watchdog időzítők"),
    ("web_ui", "Open web console", "Webkonzol megnyitása"),
    ("last_update", "updated", "frissítve"),
    ("benchmarked", "benchmarking", "benchmark fut"),
    ("autoscroll", "follow", "követés"),
    ("st_Running", "Running", "Fut"),
    ("st_Idle", "Idle", "Tétlen"),
    ("st_Failed", "Failed", "Hibás"),
    ("st_RunningFailed", "Failed", "Hibás"),
];

pub fn tr(lang: Lang, key: &str) -> &'static str {
    match TEXTS.iter().find(|(k, _, _)| *k == key) {
        Some((_, en, hu)) => {
            if lang == Lang::Hu {
                hu
            } else {
                en
            }
        }
        None => "?",
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn unique_keys() {
        let mut k: Vec<&str> = super::TEXTS.iter().map(|t| t.0).collect();
        k.sort();
        let n = k.len();
        k.dedup();
        assert_eq!(n, k.len());
    }
}
