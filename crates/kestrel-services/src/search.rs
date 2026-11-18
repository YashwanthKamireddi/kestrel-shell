//! Finding files by name, directly (no `fd` processes): the walker fd and ripgrep are built
//! on, with fd's defaults: hidden files skipped unless asked, `.gitignore` (inside a git
//! repository), `.ignore` and `.fdignore` respected, every core used, stopped as soon as
//! enough is found. The typed text matches a file's name anywhere in it, ignoring case (fd
//! reads it as a regular expression; a search box should not).

use ignore::{WalkBuilder, WalkState};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub max_results: usize,
    pub max_depth: Option<usize>,
    /// Files only (no folders).
    pub files_only: bool,
    pub hidden: bool,
    /// Stay on the drive it starts on (from `/`, not into /proc, /sys or other disks).
    pub one_file_system: bool,
}

/// The files (and folders, unless `files_only`) below `dir` whose name contains `query`.
pub fn find(dir: &Path, query: &str, o: Options) -> Vec<PathBuf> {
    let q = query.to_lowercase();
    if q.is_empty() || o.max_results == 0 {
        return Vec::new();
    }
    let found = Mutex::new(Vec::new());
    let full = AtomicBool::new(false);
    WalkBuilder::new(dir)
        .hidden(!o.hidden)
        .max_depth(o.max_depth)
        .same_file_system(o.one_file_system)
        .add_custom_ignore_filename(".fdignore")
        .threads(std::thread::available_parallelism().map_or(4, usize::from))
        .build_parallel()
        .run(|| {
            Box::new(|entry| {
                if full.load(Ordering::Relaxed) {
                    return WalkState::Quit;
                }
                let Ok(e) = entry else {
                    return WalkState::Continue;
                };
                // The starting folder itself is not a result.
                if e.depth() == 0 {
                    return WalkState::Continue;
                }
                let is_file = e.file_type().is_some_and(|t| !t.is_dir());
                if (is_file || !o.files_only)
                    && e.file_name().to_string_lossy().to_lowercase().contains(&q)
                {
                    let mut f = found
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if f.len() < o.max_results {
                        f.push(e.into_path());
                    }
                    if f.len() >= o.max_results {
                        full.store(true, Ordering::Relaxed);
                        return WalkState::Quit;
                    }
                }
                WalkState::Continue
            })
        });
    found
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_by_name_respecting_ignore_files_and_hidden() {
        let d = std::env::temp_dir().join(format!("kestrel-search-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("sub/deeper")).unwrap();
        std::fs::create_dir_all(d.join(".hid")).unwrap();
        std::fs::write(d.join("Report.PDF"), "").unwrap();
        std::fs::write(d.join("sub/deeper/report-2.txt"), "").unwrap();
        std::fs::write(d.join(".hid/report-hidden"), "").unwrap();
        std::fs::write(d.join("sub/report-ignored"), "").unwrap();
        std::fs::write(d.join(".fdignore"), "report-ignored\n").unwrap();
        let o = Options {
            max_results: 10,
            max_depth: None,
            files_only: true,
            hidden: false,
            one_file_system: true,
        };
        let mut names: Vec<String> = find(&d, "REPORT", o)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec!["Report.PDF", "report-2.txt"]);
        let hidden = find(&d, "report", Options { hidden: true, ..o });
        assert_eq!(hidden.len(), 3);
        assert_eq!(
            find(
                &d,
                "report",
                Options {
                    max_results: 1,
                    ..o
                }
            )
            .len(),
            1
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[cfg(test)]
mod live {
    /// The same search here and with fd on the home folder (by hand: `--ignored`).
    #[test]
    #[ignore]
    fn finds_what_fd_finds() {
        let home = std::path::PathBuf::from(std::env::var("HOME").unwrap());
        for q in ["readme", "kestrel", "cargo.toml"] {
            let o = super::Options {
                max_results: 100_000,
                max_depth: Some(6),
                files_only: true,
                hidden: false,
                one_file_system: false,
            };
            let t = std::time::Instant::now();
            let ours = super::find(&home, q, o).len();
            let ours_t = t.elapsed();
            let t = std::time::Instant::now();
            let fd = std::process::Command::new("fd")
                .args([
                    "--type",
                    "f",
                    "--ignore-case",
                    "--fixed-strings",
                    "--max-depth",
                    "6",
                    "--",
                    q,
                ])
                .arg(&home)
                .output()
                .map(|o| {
                    o.stdout
                        .split(|b| *b == b'\n')
                        .filter(|l| !l.is_empty())
                        .count()
                })
                .unwrap_or(0);
            println!(
                "{q}: ours {ours} in {ours_t:?}, fd {fd} in {:?}",
                t.elapsed()
            );
        }
    }
}
