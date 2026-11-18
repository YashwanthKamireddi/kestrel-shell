//! Pending package updates, asked of pacman's own library (libalpm) and the AUR's web API: no
//! `checkupdates`, `fakeroot`, `pacman` or `yay` processes.
//!
//! Repo updates are found as checkupdates finds them: the sync databases are downloaded into a
//! temporary database folder of their own (the same one checkupdates uses), whose `local` is a
//! link to the system's installed-package database, so the system's own databases are never
//! touched; then every installed package is compared with the newest the repos offer, ignored
//! packages left out (as `pacman -Qu` marks them). AUR updates are found as `yay -Qua` finds
//! them: installed packages no repo has, compared with the AUR's versions by pacman's rules.
//!
//! `/etc/pacman.conf` is read as pacman reads it: sections, `Include` (files that may open
//! sections of their own), `$repo` and `$arch` in servers (and CachyOS's `$arch_vN`), signature
//! levels, ignored packages and groups.

use alpm::{Alpm, SigLevel};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Repo {
    pub name: String,
    pub servers: Vec<String>,
    pub siglevel: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Conf {
    pub dbpath: PathBuf,
    pub gpgdir: PathBuf,
    pub arch: String,
    pub siglevel: Option<String>,
    pub ignore_pkgs: Vec<String>,
    pub ignore_groups: Vec<String>,
    pub parallel: u32,
    pub repos: Vec<Repo>,
}

fn machine() -> String {
    // Safety: uname fills a zeroed struct.
    let mut u: libc::utsname = unsafe { std::mem::zeroed() };
    if unsafe { libc::uname(&mut u) } != 0 {
        return "x86_64".into();
    }
    // Safety: machine is NUL-terminated by uname.
    unsafe { std::ffi::CStr::from_ptr(u.machine.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

/// `$repo`, `$arch` and `$arch_vN` (CachyOS: the arch's microarchitecture level) in a server.
fn expand(server: &str, repo: &str, arch: &str) -> String {
    let mut s = server.replace("$repo", repo);
    while let Some(i) = s.find("$arch_v") {
        let rest = &s[i + "$arch_v".len()..];
        let n: String = rest.chars().take_while(char::is_ascii_digit).collect();
        s = format!("{}{arch}_v{n}{}", &s[..i], &rest[n.len()..]);
    }
    s.replace("$arch", arch)
}

/// The files an `Include` names (a pattern is matched in its folder, sorted, as glob(3) does).
fn included(pattern: &str) -> Vec<PathBuf> {
    let p = Path::new(pattern);
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !name.contains(['*', '?', '[']) {
        return vec![p.to_path_buf()];
    }
    let dir = p.parent().unwrap_or(Path::new("/"));
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|f| {
                    f.file_name()
                        .is_some_and(|n| crate::open::glob_matches(&name, &n.to_string_lossy()))
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn read_into(path: &Path, conf: &mut Conf, section: &mut Option<String>, depth: u32) {
    if depth > 10 {
        return;
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let name = name.trim().to_string();
            if name != "options" && !conf.repos.iter().any(|r| r.name == name) {
                conf.repos.push(Repo {
                    name: name.clone(),
                    ..Repo::default()
                });
            }
            *section = Some(name);
            continue;
        }
        let (key, value) = match line.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None => (line, ""),
        };
        if key == "Include" {
            for f in included(value) {
                read_into(&f, conf, section, depth + 1);
            }
            continue;
        }
        match section.as_deref() {
            Some("options") => match key {
                "DBPath" => conf.dbpath = PathBuf::from(value),
                "GPGDir" => conf.gpgdir = PathBuf::from(value),
                "Architecture" => {
                    if let Some(a) = value.split_whitespace().next() {
                        conf.arch = a.to_string();
                    }
                }
                "SigLevel" => conf.siglevel = Some(value.to_string()),
                "IgnorePkg" => conf
                    .ignore_pkgs
                    .extend(value.split_whitespace().map(String::from)),
                "IgnoreGroup" => {
                    conf.ignore_groups
                        .extend(value.split_whitespace().map(String::from));
                }
                "ParallelDownloads" => conf.parallel = value.parse().unwrap_or(conf.parallel),
                _ => {}
            },
            Some(repo) => {
                if let Some(r) = conf.repos.iter_mut().find(|r| r.name == repo) {
                    match key {
                        "Server" => r.servers.push(value.to_string()),
                        "SigLevel" => r.siglevel = Some(value.to_string()),
                        _ => {}
                    }
                }
            }
            None => {}
        }
    }
}

/// `/etc/pacman.conf`, read as pacman reads it.
pub fn conf() -> Conf {
    conf_from(Path::new("/etc/pacman.conf"))
}

fn conf_from(path: &Path) -> Conf {
    let mut c = Conf {
        dbpath: PathBuf::from("/var/lib/pacman/"),
        gpgdir: PathBuf::from("/etc/pacman.d/gnupg/"),
        arch: "auto".into(),
        parallel: 1,
        ..Conf::default()
    };
    read_into(path, &mut c, &mut None, 0);
    if c.arch == "auto" {
        c.arch = machine();
    }
    let arch = c.arch.clone();
    for r in &mut c.repos {
        let name = r.name.clone();
        for s in &mut r.servers {
            *s = expand(s, &name, &arch);
        }
    }
    c
}

/// A SigLevel line, for the database side (the only side used here: nothing is installed).
/// pacman's default when none is given: `Required DatabaseOptional`.
fn siglevel(line: Option<&str>) -> SigLevel {
    let mut db = (true, true); // (checked, optional)
    for w in line
        .unwrap_or("Required DatabaseOptional")
        .split_whitespace()
    {
        let (scope_pkg, word) = if let Some(w) = w.strip_prefix("Package") {
            (true, w)
        } else if let Some(w) = w.strip_prefix("Database") {
            (false, w)
        } else {
            (false, w)
        };
        if scope_pkg {
            continue;
        }
        match word {
            "Never" => db = (false, false),
            "Optional" => db = (true, true),
            "Required" => db = (true, false),
            _ => {}
        }
    }
    match db {
        (false, _) => SigLevel::empty(),
        (true, true) => SigLevel::DATABASE | SigLevel::DATABASE_OPTIONAL,
        (true, false) => SigLevel::DATABASE,
    }
}

/// One pending update: name, version now, version offered, from the AUR.
#[derive(Clone, Debug, PartialEq)]
pub struct Update {
    pub name: String,
    pub from: String,
    pub to: String,
    pub aur: bool,
}

/// checkupdates' own database folder (shared with it: one download serves both).
fn check_db() -> PathBuf {
    let tmp = std::env::var_os("TMPDIR").map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
    // Safety: getuid cannot fail.
    tmp.join(format!("checkup-db-{}", unsafe { libc::getuid() }))
}

/// What the repos say: their updates, and the installed packages no repo has (name, version:
/// the AUR's to check).
pub struct RepoCheck {
    pub updates: Vec<Update>,
    pub foreign: Vec<(String, String)>,
}

/// The repos' updates. None if the databases could not be brought up to date (offline):
/// nothing is known then.
pub fn repo_updates() -> Option<RepoCheck> {
    let c = conf();
    let dir = check_db();
    std::fs::create_dir_all(&dir).ok()?;
    let local = dir.join("local");
    if std::fs::symlink_metadata(&local).is_err() {
        std::os::unix::fs::symlink(c.dbpath.join("local"), &local).ok()?;
    }
    let mut h = Alpm::new("/", dir.to_str()?).ok()?;
    let _ = h.set_gpgdir(c.gpgdir.to_string_lossy().as_bytes());
    h.set_disable_sandbox_filesystem(true);
    h.set_parallel_downloads(c.parallel.max(1));
    let default = siglevel(c.siglevel.as_deref());
    let _ = h.set_default_siglevel(default);
    for r in &c.repos {
        let level = r.siglevel.as_deref().map_or(default, |l| siglevel(Some(l)));
        let Ok(db) = h.register_syncdb_mut(r.name.as_str(), level) else {
            continue;
        };
        for s in &r.servers {
            let _ = db.add_server(s.as_str());
        }
    }
    if let Err(e) = h.syncdbs_mut().update(false) {
        log::warn!("updates: the package databases could not be refreshed: {e}");
        return None;
    }
    let ignored = |p: &alpm::Package| {
        c.ignore_pkgs.iter().any(|i| i == p.name())
            || p.groups()
                .iter()
                .any(|g| c.ignore_groups.iter().any(|i| i == g))
    };
    let mut repo = Vec::new();
    let mut foreign = Vec::new();
    for p in h.localdb().pkgs() {
        if let Some(n) = p.sync_new_version(h.syncdbs()) {
            if !ignored(n) {
                repo.push(Update {
                    name: p.name().to_string(),
                    from: p.version().to_string(),
                    to: n.version().to_string(),
                    aur: false,
                });
            }
        } else if !h.syncdbs().iter().any(|db| db.pkg(p.name()).is_ok()) {
            foreign.push((p.name().to_string(), p.version().to_string()));
        }
    }
    repo.sort_by(|a, b| a.name.cmp(&b.name));
    Some(RepoCheck {
        updates: repo,
        foreign,
    })
}

/// The installed packages (`foreign`: name, version) the AUR has newer, asked of its web API
/// (`/rpc/v5/info`, in batches). None if the AUR could not be reached.
pub async fn aur_updates(foreign: &[(String, String)]) -> Option<Vec<Update>> {
    let client = crate::web::client();
    let mut out = Vec::new();
    for batch in foreign.chunks(150) {
        let enc = |n: &str| -> String {
            n.bytes()
                .map(|b| match b {
                    b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'.' | b'_' => {
                        (b as char).to_string()
                    }
                    _ => format!("%{b:02X}"),
                })
                .collect()
        };
        let args: Vec<String> = batch
            .iter()
            .map(|(n, _)| format!("arg[]={}", enc(n)))
            .collect();
        let url = format!("https://aur.archlinux.org/rpc/v5/info?{}", args.join("&"));
        let v: serde_json::Value = match client.get(url).send().await {
            Ok(r) => match r.json().await {
                Ok(v) => v,
                Err(e) => {
                    log::warn!("updates: the AUR's answer could not be read: {e}");
                    return None;
                }
            },
            Err(e) => {
                log::warn!("updates: the AUR could not be reached: {e}");
                return None;
            }
        };
        for r in v["results"].as_array().into_iter().flatten() {
            let (Some(name), Some(to)) = (r["Name"].as_str(), r["Version"].as_str()) else {
                continue;
            };
            let Some((_, from)) = batch.iter().find(|(n, _)| n == name) else {
                continue;
            };
            if alpm::vercmp(to, from.as_str()) == std::cmp::Ordering::Greater {
                out.push(Update {
                    name: name.to_string(),
                    from: from.clone(),
                    to: to.to_string(),
                    aur: true,
                });
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn servers_are_expanded() {
        assert_eq!(
            expand("https://m/$repo/os/$arch", "core", "x86_64"),
            "https://m/core/os/x86_64"
        );
        assert_eq!(
            expand("https://m/repo/$arch_v3/$repo", "cachyos-v3", "x86_64"),
            "https://m/repo/x86_64_v3/cachyos-v3"
        );
    }

    #[test]
    fn includes_may_open_sections() {
        let d = std::env::temp_dir().join(format!("kestrel-pacman-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("mirrors"), "Server = https://a/$repo/$arch\n").unwrap();
        std::fs::write(
            d.join("extra.conf"),
            "[other]\nSigLevel = Never\nServer = https://o/$arch\n",
        )
        .unwrap();
        std::fs::write(
            d.join("pacman.conf"),
            format!(
                "[options]\nArchitecture = x86_64\nIgnorePkg = a b\n[core]\nInclude = {m}\n[multilib]\nInclude = {m}\nInclude = {e}\n",
                m = d.join("mirrors").display(),
                e = d.join("extra.conf").display()
            ),
        )
        .unwrap();
        let c = conf_from(&d.join("pacman.conf"));
        let names: Vec<&str> = c.repos.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["core", "multilib", "other"]);
        assert_eq!(c.repos[2].servers, ["https://o/x86_64"]);
        assert_eq!(c.repos[2].siglevel.as_deref(), Some("Never"));
        assert_eq!(c.ignore_pkgs, ["a", "b"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn signature_levels() {
        assert_eq!(
            siglevel(None),
            SigLevel::DATABASE | SigLevel::DATABASE_OPTIONAL
        );
        assert_eq!(
            siglevel(Some("Required DatabaseRequired TrustedOnly")),
            SigLevel::DATABASE
        );
        assert_eq!(siglevel(Some("Never")), SigLevel::empty());
    }
}

#[cfg(test)]
mod live {
    /// Every repository's servers as pacman-conf resolves them (by hand: `--ignored`).
    #[test]
    #[ignore]
    fn same_servers_as_pacman_conf() {
        let c = super::conf();
        for r in &c.repos {
            let out = std::process::Command::new("pacman-conf")
                .args(["-r", &r.name])
                .output()
                .unwrap();
            let theirs: Vec<String> = String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter_map(|l| l.strip_prefix("Server = ").map(String::from))
                .collect();
            assert_eq!(r.servers, theirs, "{}", r.name);
            println!("{}: {} servers, same", r.name, theirs.len());
        }
    }
}

#[cfg(test)]
mod live_updates {
    struct Log;
    impl log::Log for Log {
        fn enabled(&self, _: &log::Metadata) -> bool {
            true
        }
        fn log(&self, r: &log::Record) {
            println!("LOG {}", r.args());
        }
        fn flush(&self) {}
    }

    /// Pending updates here, by name (by hand: `--ignored`; compare with checkupdates, yay -Qua).
    #[tokio::test]
    #[ignore]
    async fn pending_updates() {
        let _ = log::set_logger(&Log).map(|()| log::set_max_level(log::LevelFilter::Warn));
        let t = std::time::Instant::now();
        let super::RepoCheck {
            updates: repo,
            foreign,
        } = tokio::task::spawn_blocking(super::repo_updates)
            .await
            .unwrap()
            .expect("databases refreshed");
        println!("repo {} in {:?}", repo.len(), t.elapsed());
        for u in &repo {
            println!("R {} {} -> {}", u.name, u.from, u.to);
        }
        let _ = rustls::crypto::ring::default_provider().install_default();
        let aur = super::aur_updates(&foreign).await.unwrap();
        // A package pretended ancient is offered; one pretended from the future is not.
        let probe = super::aur_updates(&[
            ("yay".into(), "0.0.1-1".into()),
            ("paru".into(), "999-1".into()),
        ])
        .await
        .unwrap();
        println!(
            "probe {:?}",
            probe.iter().map(|u| (&u.name, &u.to)).collect::<Vec<_>>()
        );
        assert_eq!(probe.len(), 1);
        assert_eq!(probe[0].name, "yay");
        println!("foreign {}, aur {}", foreign.len(), aur.len());
        for u in &aur {
            println!("A {} {} -> {}", u.name, u.from, u.to);
        }
    }
}
