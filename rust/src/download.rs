//! Miner downloader (Scripts/Downloader.ps1 + Expand-WebRequest): download, verify (minersha256.json),
//! extract (zip, tar.gz natively; 7z/tar.xz/... with 7-Zip or tar) and install into .\Bin\<Folder>,
//! keeping backups and the protected miner config files. Runs in a background thread.
#![allow(dead_code)]

use crate::jsonx;
use crate::util;
use crate::web;
use crate::{log_info, log_warn};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct DownloadItem {
    pub uri: String,
    /// path of the miner executable (or prerequisite file)
    pub path: PathBuf,
    pub is_miner: bool,
}

#[derive(Debug, Default)]
pub struct DownloaderState {
    pub running: bool,
    pub queue: Vec<DownloadItem>,
    pub current: Option<String>,
    pub failed: HashSet<String>,
    pub done: usize,
}

pub type SharedDownloader = Arc<Mutex<DownloaderState>>;

/// Get-MinerInstPath: ".\Bin\Folder\miner.exe" -> "<base>/Bin/Folder"
pub fn miner_inst_path(base: &Path, path: &Path) -> PathBuf {
    let bin = base.join("Bin");
    if let Ok(rel) = path.strip_prefix(&bin) {
        if let Some(first) = rel.components().next() {
            return bin.join(first.as_os_str());
        }
    }
    path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| bin.clone())
}

/// the URI stored in Bin\<Folder>\_uri.json
pub fn installed_uri(base: &Path, path: &Path) -> Option<String> {
    let f = miner_inst_path(base, path).join("_uri.json");
    let v = util::read_text(&f).ok().and_then(|t| jsonx::parse(&t))?;
    let u = jsonx::s(jsonx::get(&v, "URI"));
    if u.is_empty() {
        None
    } else {
        Some(u)
    }
}

fn sevenzip(base: &Path) -> Option<PathBuf> {
    let mut c: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        c.push(base.join("7z.exe"));
        c.push(base.join("Includes").join("dist").join("7z.exe"));
        c.push(base.join("Includes").join("7z.exe"));
    } else {
        let arch = match std::env::consts::ARCH {
            "aarch64" => "arm64",
            "x86" => "i386",
            _ => "amd64",
        };
        c.push(base.join("IncludesLinux").join("bin").join(format!("7zz-{}", arch)));
        for n in ["7zz", "7z", "7za"] {
            if Command::new(n).arg("i").output().is_ok() {
                c.push(PathBuf::from(n));
            }
        }
    }
    c.into_iter().find(|p| p.components().count() == 1 || p.exists())
}

fn extract_zip(file: &Path, to: &Path) -> Result<()> {
    let f = std::fs::File::open(file)?;
    let mut z = zip::ZipArchive::new(f)?;
    for i in 0..z.len() {
        let mut e = z.by_index(i)?;
        let Some(name) = e.enclosed_name() else { continue };
        let out = to.join(name);
        if e.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p)?;
        }
        let mut o = std::fs::File::create(&out)?;
        std::io::copy(&mut e, &mut o)?;
        #[cfg(unix)]
        if let Some(mode) = e.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode));
        }
    }
    Ok(())
}

fn extract_tgz(file: &Path, to: &Path) -> Result<()> {
    let f = std::fs::File::open(file)?;
    let gz = flate2::read::GzDecoder::new(f);
    let mut ar = tar::Archive::new(gz);
    ar.set_preserve_permissions(true);
    ar.unpack(to)?;
    Ok(())
}

fn extract(base: &Path, file: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    let name = file.file_name().map(|f| f.to_string_lossy().to_lowercase()).unwrap_or_default();
    if name.ends_with(".zip") {
        return extract_zip(file, to);
    }
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        return extract_tgz(file, to);
    }
    if name.contains(".tar.") && cfg!(unix) {
        let st = Command::new("tar").arg("-xa").arg("-f").arg(file).arg("-C").arg(to).status()?;
        if st.success() {
            return Ok(());
        }
    }
    let sz = sevenzip(base).ok_or_else(|| anyhow!("7-Zip is needed to extract {}", name))?;
    let st = Command::new(&sz)
        .arg("x")
        .arg(file)
        .arg(format!("-o{}", to.display()))
        .arg("-y")
        .stdout(std::process::Stdio::null())
        .status()?;
    if !st.success() {
        return Err(anyhow!("7-Zip failed on {}", name));
    }
    // a .tar inside the 7z/xz
    if name.contains(".tar.") {
        if let Ok(rd) = std::fs::read_dir(to) {
            for e in rd.flatten() {
                if e.path().extension().and_then(|x| x.to_str()) == Some("tar") {
                    let tf = e.path();
                    let f = std::fs::File::open(&tf)?;
                    tar::Archive::new(f).unpack(to)?;
                    let _ = std::fs::remove_file(&tf);
                }
            }
        }
    }
    Ok(())
}

fn move_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    match std::fs::rename(from, to) {
        Ok(_) => Ok(()),
        Err(_) => {
            copy_dir(from, to)?;
            std::fs::remove_dir_all(from)
        }
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)?.flatten() {
        let p = e.path();
        let t = to.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &t)?;
        } else {
            std::fs::copy(&p, &t)?;
        }
    }
    Ok(())
}

fn protected_files(base: &Path) -> Vec<String> {
    let mut v = jsonx::str_list(&util::read_text(&base.join("Data").join("protectedminerfiles.json")).ok().and_then(|t| jsonx::parse(&t)).unwrap_or(Value::Null));
    if let Ok(t) = util::read_text(&base.join("Config").join("minerconfigfiles.txt")) {
        for line in t.lines() {
            let l = line.split(';').next().unwrap_or("").trim();
            if !l.is_empty() {
                v.push(l.to_string());
            }
        }
    }
    v
}

/// Expand-WebRequest for one miner
pub fn install(base: &Path, item: &DownloadItem, keep_downloads: bool, backups: bool) -> Result<()> {
    let leaf = item.uri.rsplit('/').next().unwrap_or("download").split('?').next().unwrap_or("download").to_string();
    let downloads = base.join("Downloads");
    std::fs::create_dir_all(&downloads)?;
    std::fs::create_dir_all(base.join("Bin"))?;
    let file = downloads.join(&leaf);
    if file.exists() {
        let _ = std::fs::remove_file(&file);
    }
    log_info!("Downloading {}", item.uri);
    web::download_file(&item.uri, &file, 600)?;

    let sha_db = util::read_text(&base.join("Data").join("minersha256.json")).ok().and_then(|t| jsonx::parse(&t)).unwrap_or(Value::Null);
    let sha = jsonx::s(jsonx::get(&sha_db, &item.uri));
    if !sha.is_empty() {
        let h = util::sha256_file(&file)?;
        if !h.eq_ignore_ascii_case(&sha) {
            let _ = std::fs::remove_file(&file);
            return Err(anyhow!("Downloadfile {} has wrong hash! Please open an issue at github.com.", file.display()));
        }
    }

    // a plain binary (not an archive): copy it next to the path
    if !item.is_miner && Path::new(&leaf).file_name() == item.path.file_name() {
        if let Some(p) = item.path.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::copy(&file, &item.path)?;
        return Ok(());
    }
    let ext = Path::new(&leaf).extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
    if ext == "exe" || ext == "msi" {
        return Err(anyhow!("installers (.exe/.msi) are not run by the Rust port, install {} manually", leaf));
    }

    let target = if item.is_miner { miner_inst_path(base, &item.path) } else { item.path.parent().unwrap_or(base).to_path_buf() };
    let parent = target.parent().unwrap_or(base).to_path_buf();
    let stem = leaf.trim_end_matches(".gz").trim_end_matches(".xz").trim_end_matches(".bz2").trim_end_matches(".tar").trim_end_matches(".zip").trim_end_matches(".7z").trim_end_matches(".tgz").to_string();
    let tmp = parent.join(format!("{}.extract", stem));
    if tmp.exists() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    extract(base, &file, &tmp)?;

    if item.is_miner {
        let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
        let leafname = target.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
        let bak = parent.join(format!("{}.{}", leafname, stamp));
        if target.exists() {
            move_dir(&target, &bak)?;
        }
        let has_files = std::fs::read_dir(&tmp)?.flatten().any(|e| e.path().is_file());
        if has_files {
            move_dir(&tmp, &target)?;
        } else {
            let dirs: Vec<PathBuf> = std::fs::read_dir(&tmp)?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
            for (i, d) in dirs.iter().enumerate() {
                if i == 0 {
                    move_dir(d, &target)?;
                } else {
                    move_dir(d, &target.join(d.file_name().unwrap()))?;
                }
            }
            let _ = std::fs::remove_dir_all(&tmp);
        }
        // restore protected config files from the backup
        if bak.exists() {
            for pat in protected_files(base) {
                let pat_path = Path::new(&pat);
                let (sub, fpat) = match pat_path.parent() {
                    Some(p) if !p.as_os_str().is_empty() => (p.to_path_buf(), pat_path.file_name().unwrap().to_string_lossy().to_string()),
                    _ => (PathBuf::new(), pat.clone()),
                };
                if let Ok(rd) = std::fs::read_dir(bak.join(&sub)) {
                    for e in rd.flatten() {
                        let n = e.file_name().to_string_lossy().to_string();
                        let l = n.to_lowercase();
                        if !e.path().is_file() || l.ends_with(".dll") || l.ends_with(".exe") || l.ends_with(".bin") {
                            continue;
                        }
                        if crate::device::like(&n, &fpat) {
                            let dst = target.join(&sub);
                            let _ = std::fs::create_dir_all(&dst);
                            let _ = std::fs::copy(e.path(), dst.join(&n));
                        }
                    }
                }
            }
            let _ = std::fs::remove_dir_all(bak.join("DAGs"));
            // keep at most 3 backups (or none)
            let keep = if backups { 3 } else { 0 };
            let mut baks: Vec<PathBuf> = std::fs::read_dir(&parent)?
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir() && p.file_name().map(|f| f.to_string_lossy().starts_with(&format!("{}.", leafname))).unwrap_or(false))
                .collect();
            baks.sort();
            baks.reverse();
            for b in baks.into_iter().skip(keep) {
                let _ = std::fs::remove_dir_all(b);
            }
        }
        let _ = std::fs::create_dir_all(&target);
        let _ = crate::config::write_json(&target.join("_uri.json"), &json!({"URI": item.uri}));
        #[cfg(unix)]
        if item.path.exists() {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&item.path, std::fs::Permissions::from_mode(0o755));
        }
    } else {
        std::fs::create_dir_all(&target)?;
        for e in std::fs::read_dir(&tmp)?.flatten() {
            let to = target.join(e.file_name());
            if e.path().is_dir() {
                let _ = move_dir(&e.path(), &to);
            } else {
                let _ = std::fs::rename(e.path(), &to).or_else(|_| std::fs::copy(e.path(), &to).map(|_| ()));
            }
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }
    if !keep_downloads {
        let _ = std::fs::remove_file(&file);
    }
    Ok(())
}

/// start the background downloader for the list (skips paths of running miners)
pub fn start(state: &SharedDownloader, base: PathBuf, items: Vec<DownloadItem>, running_paths: Vec<PathBuf>, keep_downloads: bool, backups: bool) {
    {
        let mut st = state.lock().unwrap();
        if st.running {
            return;
        }
        st.running = true;
        st.queue = items.clone();
    }
    let state = state.clone();
    std::thread::spawn(move || {
        for item in items {
            if running_paths.iter().any(|p| p == &item.path) {
                continue;
            }
            if item.is_miner && item.path.exists() && installed_uri(&base, &item.path).as_deref() == Some(item.uri.as_str()) {
                continue;
            }
            state.lock().unwrap().current = Some(item.uri.clone());
            match install(&base, &item, keep_downloads, backups) {
                Ok(_) => {
                    log_info!("Downloaded and installed {}", item.uri);
                    state.lock().unwrap().done += 1;
                }
                Err(e) => {
                    log_warn!("Downloader-error: {}", e);
                    log_warn!("Cannot download {} distributed at {}. ", item.path.display(), item.uri);
                    state.lock().unwrap().failed.insert(item.uri.clone());
                }
            }
        }
        let mut st = state.lock().unwrap();
        st.running = false;
        st.current = None;
        st.queue.clear();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inst_path() {
        let base = PathBuf::from("/rbm");
        assert_eq!(miner_inst_path(&base, Path::new("/rbm/Bin/NVIDIA-Trex/t-rex")), PathBuf::from("/rbm/Bin/NVIDIA-Trex"));
        assert_eq!(miner_inst_path(&base, Path::new("/rbm/Bin/CPU-Xmrig/sub/xmrig")), PathBuf::from("/rbm/Bin/CPU-Xmrig"));
    }
}
