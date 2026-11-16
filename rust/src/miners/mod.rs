//! Miner modules (Miners/*.ps1). The command tables are the original ones, extracted to
//! assets/miners/<Name>.json; the module code builds the command lines like the PowerShell code.

pub mod bzminer;
pub mod cpuminer_jayddee;
pub mod custom;
pub mod fam_amd;
pub mod fam_cpu;
pub mod fam_misc;
pub mod fam_nvidia;
pub mod fam_progpow;
pub mod fam_versions;
pub mod fam_wildrig;
pub mod family;
pub mod gminer;
pub mod lolminer;
pub mod onezero;
pub mod rigel;
pub mod srbminermulti;
pub mod trex;
pub mod xmrig;

use crate::miner::MinerModule;
use crate::pool::Pool;

pub fn registry() -> Vec<Box<dyn MinerModule>> {
    let mut v: Vec<Box<dyn MinerModule>> = vec![
        Box::new(bzminer::BzMiner),
        Box::new(cpuminer_jayddee::CpuminerJayddee),
        Box::new(gminer::Gminer),
        Box::new(lolminer::LolMiner),
        Box::new(onezero::OneZeroMiner),
        Box::new(rigel::Rigel),
        Box::new(srbminermulti::SrbMinerMulti),
        Box::new(trex::Trex),
        Box::new(xmrig::Xmrig { cc: false }),
        Box::new(xmrig::Xmrig { cc: true }),
    ];
    for m in fam_nvidia::modules().into_iter().chain(fam_amd::modules()).chain(fam_cpu::modules()).chain(fam_progpow::modules()).chain(fam_misc::modules()).chain(fam_misc::modules_trex19()).chain(fam_wildrig::modules()).chain(fam_versions::modules()) {
        v.push(Box::new(m));
    }
    // Get-ChildItem .\Miners\*.ps1 order
    v.sort_by_key(|m| m.name().to_lowercase());
    v
}

pub fn names() -> Vec<String> {
    registry().iter().map(|m| m.name().to_string()).collect()
}

pub fn ssl_word(p: &Pool) -> &'static str {
    if p.SSL {
        "ssl"
    } else {
        "tcp"
    }
}

/// " -o <proto>://<host>:<port> -u <user> [-p <pass>]" for each failover pool (Trex/Ccminer style)
pub fn failover_args(p: &Pool) -> String {
    p.Failover
        .iter()
        .map(|f| {
            format!(
                " -o {}://{}:{} -u {}{}",
                f.Protocol,
                f.Host,
                f.Port,
                f.User,
                if f.Pass.is_empty() { String::new() } else { format!(" -p {}", f.Pass) }
            )
        })
        .collect()
}

/// host[:port] unless the host already contains a path
pub fn host_port(p: &Pool, port: u32) -> String {
    if port > 0 && !p.Host.contains('/') {
        format!("{}:{}", p.Host, port)
    } else {
        p.Host.clone()
    }
}

pub fn opt(cond: bool, s: String) -> String {
    if cond {
        s
    } else {
        String::new()
    }
}

#[cfg(test)]
pub mod tests;
