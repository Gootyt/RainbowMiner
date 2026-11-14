//! The console output of a round (the "Start output to host" part of Invoke-Core):
//! profit tables per device, status, active miners, watchdog timers, balances, profit line.

use crate::data;
use crate::device;
use crate::engine::Core;
use crate::jsonx;
use crate::log::{self, Color};
use crate::miner::MinerCandidate;
use crate::minerapi::MinerStatus;
use crate::rates;
use crate::util;

/// a column: label and right alignment
pub struct Col {
    pub label: String,
    pub right: bool,
}

fn col(l: &str, right: bool) -> Col {
    Col { label: l.to_string(), right }
}

/// Format-Table: rows of cells, a cell may hold several lines
pub fn table(cols: &[Col], rows: &[Vec<Vec<String>>]) -> Vec<String> {
    let n = cols.len();
    let mut w: Vec<usize> = cols.iter().map(|c| c.label.chars().count()).collect();
    for r in rows {
        for (i, cell) in r.iter().enumerate().take(n) {
            for l in cell {
                w[i] = w[i].max(l.chars().count());
            }
        }
    }
    let fmt = |s: &str, i: usize| -> String {
        let pad = w[i].saturating_sub(s.chars().count());
        if cols[i].right {
            format!("{}{}", " ".repeat(pad), s)
        } else {
            format!("{}{}", s, " ".repeat(pad))
        }
    };
    let mut out = Vec::new();
    out.push(String::new());
    out.push(cols.iter().enumerate().map(|(i, c)| fmt(&c.label, i)).collect::<Vec<_>>().join(" ").trim_end().to_string());
    out.push(cols.iter().enumerate().map(|(i, c)| fmt(&"-".repeat(c.label.chars().count()), i)).collect::<Vec<_>>().join(" ").trim_end().to_string());
    for r in rows {
        let lines = r.iter().map(|c| c.len().max(1)).max().unwrap_or(1);
        for li in 0..lines {
            let line: Vec<String> = (0..n).map(|i| fmt(r.get(i).and_then(|c| c.get(li)).map(|s| s.as_str()).unwrap_or(""), i)).collect();
            out.push(line.join(" ").trim_end().to_string());
        }
    }
    out.push(String::new());
    out
}

fn pct(v: f64) -> String {
    let s = format!("{:.2} %", v);
    s.replace(".00 %", " %").replace("0 %", " %").replace("  %", " %")
}

fn print(lines: Vec<String>) {
    for l in lines {
        log::console(&l);
    }
}

/// the profit table of one device model group
pub fn miners_table(core: &Core, model: &str, miners: &[&MinerCandidate], limit: usize) {
    let models: Vec<&str> = model.split('-').collect();
    let mut titles: Vec<(String, String)> = Vec::new();
    for d in &core.devices.devices {
        if models.contains(&d.Model.as_str()) && !titles.iter().any(|t| t.0 == d.Model) {
            titles.push((d.Model.clone(), d.Model_Name.clone()));
        }
    }
    titles.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    let title = titles
        .iter()
        .map(|(m, n)| {
            let mut names: Vec<String> = core.devices.devices.iter().filter(|d| &d.Model == m).map(|d| d.Name.clone()).collect();
            names.sort();
            format!("{} ({})", n, names.join(","))
        })
        .collect::<Vec<_>>()
        .join(", ");
    log::console(&title);
    log::console(&"=".repeat(title.chars().count()));
    let currencies: Vec<String> = {
        let mut c = core.config.arr("Currency");
        c.sort();
        c
    };
    let mut columns = core.config.arr("UIProfitColumns");
    if columns.is_empty() {
        columns = ["Miner", "Fee", "Algorithm", "Speed", "Diff", "Power", "Profit", "TTF", "Accuracy", "Pool", "PoolFee", "Wallet"].iter().map(|s| s.to_string()).collect();
    }
    let use_pp = core.config.b("UsePowerPrice") && (core.config.f("PowerOffset") > 0.0 || core.config.f("PowerOffsetPercent") > 0.0);
    let mut cols: Vec<Col> = Vec::new();
    for c in &columns {
        match c.to_lowercase().as_str() {
            "miner" => cols.push(col("Miner", false)),
            "fee" => cols.push(col("Fee", true)),
            "algorithm" => cols.push(col("Algorithm", false)),
            "speed" => cols.push(col("Speed", true)),
            "diff" => cols.push(col("Diff", true)),
            "power" => cols.push(col(if use_pp { "Power*" } else { "Power" }, true)),
            "profit" => {
                for cur in &currencies {
                    cols.push(col(&format!("{}/Day", cur), true));
                }
            }
            "ttf" => cols.push(col("TTF", true)),
            "accuracy" => cols.push(col("Accuracy", true)),
            "pool" => cols.push(col("Pool", false)),
            "poolfee" => cols.push(col("PoolFee", true)),
            "wallet" => cols.push(col("Wallet", false)),
            _ => {}
        }
    }
    let mut rows = Vec::new();
    let re_num = regex::Regex::new(r"^\d+").unwrap();
    for m in miners.iter().take(limit) {
        let mut r: Vec<Vec<String>> = Vec::new();
        for c in &columns {
            match c.to_lowercase().as_str() {
                "miner" => r.push(vec![m.name.split('-').next().unwrap_or("").to_string()]),
                "fee" => r.push(vec![m.hashrates.keys().map(|k| m.dev_fee.get(k).copied().filter(|f| *f != 0.0).map(pct).unwrap_or_else(|| "-".into())).collect::<Vec<_>>().join(",")]),
                "algorithm" => r.push(vec![data::get_mapped_algorithm(&m.hashrates.keys().map(|k| crate::miner::strip_alternate(k)).collect::<Vec<_>>().join("-"), core.config.b("EnableAlgorithmMapping"))]),
                "speed" => r.push(
                    m.hashrates
                        .values()
                        .map(|h| match h {
                            Some(v) => format!("{}/s", util::convert_to_hash(*v)),
                            None if core.benchmarking => "Benchmarking".into(),
                            None => "Waiting".into(),
                        })
                        .collect(),
                ),
                "diff" => r.push(vec![m
                    .hashrates
                    .keys()
                    .map(|k| m.difficulties.get(k).copied().filter(|d| *d > 0.0).map(|d| util::convert_to_float(d).replace(' ', "")).unwrap_or_else(|| "-".into()))
                    .collect::<Vec<_>>()
                    .join(",")]),
                "power" => r.push(vec![format!("{}W", m.power_draw as i64)]),
                "profit" => {
                    for cur in &currencies {
                        r.push(vec![match (m.profit, rates::get(cur)) {
                            (Some(p), Some(rate)) if p != 0.0 => util::convert_to_local_currency(p, rate, 2),
                            _ => "Unknown".into(),
                        }]);
                    }
                }
                "ttf" => r.push(m.pools.values().map(|p| p.BLK.filter(|b| *b > 0.0).map(|b| util::convert_to_ttf(86400.0 / b)).unwrap_or_else(|| "-".into())).collect()),
                "accuracy" => r.push(m.pools.values().map(|p| format!("{:.0}%", (1.0 - p.MarginOfError) * 100.0)).collect()),
                "pool" => r.push(
                    m.pools
                        .values()
                        .map(|p| {
                            if re_num.is_match(&p.CoinName) {
                                format!("{}-{}", p.Name, p.CoinName)
                            } else if !p.CoinSymbol.is_empty() {
                                format!("{}-{}", p.Name, p.CoinSymbol)
                            } else {
                                p.Name.clone()
                            }
                        })
                        .collect(),
                ),
                "poolfee" => r.push(m.pools.values().map(|p| if p.PoolFee != 0.0 { pct(p.PoolFee) } else { "-".into() }).collect()),
                "wallet" => r.push(
                    m.pools
                        .values()
                        .map(|p| {
                            let w = &p.Wallet;
                            if w.is_empty() {
                                "-".into()
                            } else if w.chars().count() <= 8 {
                                w.clone()
                            } else {
                                let c: Vec<char> = w.chars().collect();
                                format!("{}..{}", c[..3].iter().collect::<String>(), c[c.len() - 3..].iter().collect::<String>())
                            }
                        })
                        .collect(),
                ),
                _ => {}
            }
        }
        rows.push(r);
    }
    print(table(&cols, &rows));
}

/// the whole round output
pub fn show_round(core: &mut Core, miners: &[MinerCandidate], needing: usize, downloading: usize, downloading_prq: usize, message: &str, have_pools: bool, wd_reset_secs: i64) {
    log::clear_console();
    if core.devices.devices.is_empty() {
        log::console_colored("No devices available. Running in pause mode, only. ", Color::Yellow);
    } else if !have_pools {
        log::console_colored("No pools available: check your configuration", Color::Yellow);
    } else if miners.is_empty() {
        log::console_colored(
            &format!("No miners available: {}", if downloading > 0 { "Downloading miners, please be patient" } else { "check your configuration" }),
            Color::Yellow,
        );
    }
    let limit = if core.ui_full || core.benchmarking { 100 } else { 3 };
    let mut models: Vec<String> = Vec::new();
    for m in miners {
        if !models.contains(&m.device_model) {
            models.push(m.device_model.clone());
        }
    }
    models.sort();
    for model in models {
        let profit_min = if model.contains("CPU") { 1e-9 } else { 1e-7 };
        let mut list: Vec<&MinerCandidate> = miners
            .iter()
            .filter(|m| m.device_model == model && (m.hashrates.values().any(|h| h.map(|x| x > 0.0).unwrap_or(false)) || m.profit.is_none()))
            .collect();
        let sort_profit = core.config.s("UIsorting").eq_ignore_ascii_case("profit");
        list.sort_by(|a, b| {
            if core.benchmarking && !sort_profit {
                let ka = a.hashrates.keys().cloned().collect::<Vec<_>>().join(",");
                let kb = b.hashrates.keys().cloned().collect::<Vec<_>>().join(",");
                ka.cmp(&kb).then(b.profit.unwrap_or(f64::MAX).partial_cmp(&a.profit.unwrap_or(f64::MAX)).unwrap_or(std::cmp::Ordering::Equal))
            } else if needing == 0 || core.is_exclusive_run {
                b.profit_bias.partial_cmp(&a.profit_bias).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                b.profit.unwrap_or(f64::MAX).partial_cmp(&a.profit.unwrap_or(f64::MAX)).unwrap_or(std::cmp::Ordering::Equal)
            }
        });
        let use_pp = core.config.b("UsePowerPrice");
        let mut shown: Vec<&MinerCandidate> = Vec::new();
        for (i, m) in list.iter().enumerate() {
            let p = m.profit.unwrap_or(0.0) + if use_pp && m.profit_cost.unwrap_or(0.0) > 0.0 { m.profit_cost.unwrap_or(0.0) } else { 0.0 };
            if i < limit.min(5) || (!core.ui_full && m.hashrates.values().any(|h| h.map(|x| x > 0.0).unwrap_or(false))) || p >= profit_min || m.profit.is_none() {
                shown.push(m);
            }
        }
        miners_table(core, &model, &shown, limit);
    }
    if core.restart_miners {
        log::console_colored("Miners have been restarted!", Color::Yellow);
        log::console(" ");
        core.restart_miners = false;
    }
    if core.is_paused() {
        log::console_colored(&format!("Status: PAUSED {} (press P to resume)", core.pause_status_text()), Color::Red);
        log::console(" ");
    } else if !core.profitable {
        log::console_colored("Status: WAITING FOR PROFITABILITY (be patient or set CheckProfitability to 0 to resume)", Color::Red);
        log::console(" ");
    } else {
        if core.lock_miners {
            log::console_colored(&format!("Status: LOCKED to {} (press L to unlock)", core.lock_pools.join(", ")), Color::Cyan);
            log::console(" ");
        }
        if core.benchmarking {
            log::console_colored(
                &format!("Benchmarking in progress: {} miner{} left, interval is set to {} seconds.", needing, if needing > 1 { "s" } else { "" }, core.config.i("BenchmarkInterval")),
                Color::Yellow,
            );
        }
        if downloading > 0 {
            log::console_colored(&format!("Download in progress: {} miner{} left.", downloading, if downloading > 1 { "s" } else { "" }), Color::Yellow);
        }
        if downloading_prq > 0 {
            log::console_colored(&format!("Download in progress: {} mining pre-requisite{} left.", downloading_prq, if downloading_prq > 1 { "s" } else { "" }), Color::Yellow);
        }
        if !message.is_empty() {
            log::console_colored(message, Color::Yellow);
        }
    }

    // active miners
    let interval = core.config.i("Interval");
    let devices = core.all_devices.clone();
    let mut rows_running: Vec<Vec<Vec<String>>> = Vec::new();
    let mut rows_other: Vec<(String, Vec<Vec<String>>)> = Vec::new();
    let first_rounds = core.config.i("MinimumMiningIntervals");
    let ui_full = core.ui_full || core.benchmarking;
    let re_num = regex::Regex::new(r"^\d+").unwrap();
    for m in core.active_miners.iter_mut() {
        let recent = m.get_active_last() > chrono::Utc::now() - chrono::Duration::seconds(5 * interval);
        if !(m.activated > 0 && (m.status == MinerStatus::Running || (!m.donator && ui_full && recent))) {
            continue;
        }
        let act = m.get_active_time();
        let mut name = m.name.split('-').next().unwrap_or("").to_string();
        if m.is_focus_wallet_miner || m.is_exclusive_miner {
            name.push_str("(!)");
        } else if m.post_block_mining > 0 {
            name.push_str(&format!("({}s)", m.post_block_mining));
        } else if first_rounds > 1 && needing == 0 && (m.is_running_first_rounds || (m.rounds == 0 && m.status == MinerStatus::Running)) {
            name.push_str(&format!("({}/{})", m.rounds + 1, first_rounds));
        }
        let coins: Vec<String> = (0..m.coin_symbol.len())
            .map(|i| {
                let cn = m.coin_name.get(i).cloned().unwrap_or_default();
                if re_num.is_match(&cn) {
                    cn
                } else if !m.coin_symbol[i].is_empty() {
                    m.coin_symbol[i].clone()
                } else {
                    "-".into()
                }
            })
            .collect();
        let row = vec![
            m.speed_live.iter().map(|s| format!("{}/s", util::convert_to_hash(*s))).collect(),
            vec![format!("{}d/{:02}h/{:02}m", act.num_days(), act.num_hours() % 24, act.num_minutes() % 60)],
            vec![match m.activated {
                0 => "Never".into(),
                1 => "Once".into(),
                n => format!("{} Times", n),
            }],
            vec![name],
            vec![data::get_mapped_algorithm(&m.base_algorithm.join("-"), core.config.b("EnableAlgorithmMapping"))],
            coins,
            vec![device::device_model_name_short(&devices, &m.device_name).join(",")],
            m.pool.clone(),
            vec![format!("{}W", m.power_draw as i64)],
            vec![format!(
                "{} {}",
                m.path.strip_prefix(&core.base).map(|p| p.display().to_string()).unwrap_or_else(|_| m.path.display().to_string()),
                m.argument_list
            )],
        ];
        if m.status == MinerStatus::Running {
            rows_running.push(row);
        } else {
            rows_other.push((m.status.as_str().to_string(), row));
        }
    }
    let cols = vec![
        col("Last Speed", true),
        col("Active", false),
        col("Started", false),
        col("Miner", false),
        col("Algorithm", false),
        col("Coin", false),
        col("Device", false),
        col("Pool", false),
        col("Power", false),
        col("Command", false),
    ];
    if !rows_running.is_empty() {
        log::console("   Status: Running");
        print(table(&cols, &rows_running));
    }
    rows_other.sort_by(|a, b| a.0.cmp(&b.0));
    let mut statuses: Vec<String> = rows_other.iter().map(|r| r.0.clone()).collect();
    statuses.dedup();
    for st in statuses {
        let rows: Vec<Vec<Vec<String>>> = rows_other.iter().filter(|r| r.0 == st).map(|r| r.1.clone()).take(6).collect();
        log::console(&format!("   Status: {}", st));
        print(table(&cols, &rows));
    }

    // watchdog timers
    if ui_full {
        let now = core.timer;
        let rows: Vec<Vec<Vec<String>>> = core
            .watchdog_timers
            .iter()
            .filter(|t| t.kicked > now - chrono::Duration::seconds(wd_reset_secs))
            .map(|t| {
                vec![
                    vec![t.miner_name.split('-').next().unwrap_or("").to_string()],
                    vec![t.device_model.clone()],
                    vec![t.pool_name.clone()],
                    vec![data::get_mapped_algorithm(&t.algorithm, core.config.b("EnableAlgorithmMapping"))],
                    vec![format!("{} Seconds", (now - t.kicked).num_seconds())],
                ]
            })
            .collect();
        if !rows.is_empty() {
            print(table(&[col("Miner", false), col("Device", false), col("Pool", false), col("Algorithm", false), col("Watchdog Timer", true)], &rows));
        }
    }
    if core.config.b("UsePowerPrice") && (core.config.f("PowerOffset") > 0.0 || core.config.f("PowerOffsetPercent") > 0.0) {
        log::console(&format!(
            "* net power consumption. A base power offset of {}{} is being added to calculate the final profit.",
            if core.config.f("PowerOffsetPercent") > 0.0 { format!("{:.1}%", core.config.f("PowerOffsetPercent")) } else { String::new() },
            if core.config.f("PowerOffset") > 0.0 { format!(" +{}W", core.config.f("PowerOffset") as i64) } else { String::new() }
        ));
        log::console(" ");
    }

    // balances
    if core.config.b("ShowPoolBalances") && core.balances.len() > 1 {
        let next = core.config.i("BalanceUpdateMinutes") - core.balances_updated.map(|t| (chrono::Utc::now() - t).num_minutes()).unwrap_or(0);
        log::console(&format!(
            "Pool Balances as of {} (next update {}): ",
            core.balances_updated.map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string()).unwrap_or_default(),
            if next > 0 { format!("in {} minutes", next) } else { "now".into() }
        ));
        let mut keys: Vec<String> = Vec::new();
        for b in &core.balances {
            for k in jsonx::keys(b) {
                if k.starts_with("Value in ") && !keys.contains(&k) {
                    keys.push(k);
                }
            }
        }
        keys.sort();
        let multi = {
            let mut c: Vec<String> = core.balances.iter().map(|b| jsonx::s(jsonx::get(b, "Currency"))).filter(|c| !c.is_empty()).collect();
            c.sort();
            c.dedup();
            c.len() > 1
        };
        let mut cols = vec![col("Name", false)];
        if multi {
            cols.push(col("Sym", false));
            cols.push(col("Balance", true));
            cols.push(col("Pending", true));
        }
        for k in &keys {
            cols.push(col(k.trim_start_matches("Value in "), true));
        }
        let rows: Vec<Vec<Vec<String>>> = core
            .balances
            .iter()
            .map(|b| {
                let cur = jsonx::s(jsonx::get(b, "Currency"));
                let mut r = vec![vec![jsonx::s(jsonx::get(b, "Name"))]];
                if multi {
                    r.push(vec![cur.clone()]);
                    r.push(vec![jsonx::s(jsonx::get(b, &format!("Balance ({})", cur)))]);
                    let p = jsonx::s(jsonx::get(b, &format!("Pending ({})", cur)));
                    r.push(vec![if p.is_empty() || p.trim_matches(['0', '.', ',']).is_empty() { "-".into() } else { p }]);
                }
                for k in &keys {
                    r.push(vec![jsonx::s(jsonx::get(b, k))]);
                }
                r
            })
            .collect();
        print(table(&cols, &rows));
    }
}

/// the profit status line ("[date] Profit = ...")
pub fn profit_line(core: &Core, profit_total: f64, without_cost: f64) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut currencies = core.config.arr("Currency");
    currencies.sort();
    let use_pp = core.config.b("UsePowerPrice");
    for cur in &currencies {
        let Some(rate) = rates::get(cur) else { continue };
        let (mut name, mut p, mut w, mut off) = (cur.clone(), profit_total, without_cost, 2);
        if cur == "BTC" && without_cost != 0.0 {
            match (without_cost.abs().ln() / 1000f64.ln()).trunc() as i64 {
                -1 => {
                    name = "mBTC".into();
                    p *= 1e3;
                    w *= 1e3;
                    off = 5;
                }
                -2 => {
                    name = "µBTC".into();
                    p *= 1e6;
                    w *= 1e6;
                    off = 8;
                }
                -3 => {
                    name = "sat".into();
                    p *= 1e8;
                    w *= 1e8;
                    off = 10;
                }
                _ => {}
            }
        }
        parts.push(format!(
            "{}{} {}/Day",
            util::convert_to_local_currency(p, rate, off),
            if use_pp { format!("/{}", util::convert_to_local_currency(w, rate, off)) } else { String::new() },
            name
        ));
    }
    let others: Vec<String> = currencies.iter().filter(|c| *c != "BTC").filter_map(|c| rates::get(c).map(|r| format!("{} {}", c, r))).collect();
    if !others.is_empty() {
        parts.push(format!("1 BTC = {}", others.join(" = ")));
    }
    if use_pp {
        parts.push(format!("E-Price = {} {}", core.config.s("PowerPriceCurrency"), util::round_to(core.current_power_price, 3)));
    }
    format!(" [{}] Profit = {} ", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), parts.join(" | "))
}
