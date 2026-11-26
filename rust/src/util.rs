//! Petits outils : texte bilingue, fichiers, journal.

use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Texte en deux langues : l'interface choisit, le journal utilise le français.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct M {
    pub fr: String,
    pub en: String,
}

pub fn m(fr: impl Into<String>, en: impl Into<String>) -> M {
    M { fr: fr.into(), en: en.into() }
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn read_json(p: &Path) -> Option<serde_json::Value> {
    let s = fs::read_to_string(p).ok()?;
    serde_json::from_str(&s).ok()
}

/// Écriture sûre : on écrit à côté puis on remplace d'un coup.
/// Un plantage pendant l'écriture ne peut pas laisser un fichier à moitié écrit.
pub fn write_atomic(p: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = p.with_extension(format!("{}.tmp", std::process::id()));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, p)
}

pub fn write_json_atomic(p: &Path, v: &impl Serialize) -> std::io::Result<()> {
    let s = serde_json::to_vec_pretty(v).map_err(std::io::Error::other)?;
    write_atomic(p, &s)
}

/// Garde au plus `n` caractères (pas des octets : les accents ne sont pas coupés en deux).
pub fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

pub fn is_discord_id(s: &str) -> bool {
    (5..=25).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit())
}

// ---------- Journal ----------
// Le .exe n'a pas de fenêtre : on écrit dans voice-overlay.log, avec rotation (un fichier .old).

const LOG_MAX: u64 = 1_000_000;
const RECENT_MAX: usize = 150;

/// Dernières lignes du journal, gardées en mémoire pour « Copier le diagnostic ».
static RECENT: Mutex<std::collections::VecDeque<String>> = Mutex::new(std::collections::VecDeque::new());

pub fn recent_log() -> Vec<String> {
    RECENT.lock().map(|r| r.iter().cloned().collect()).unwrap_or_default()
}

struct FileLogger {
    path: Option<PathBuf>,
    lock: Mutex<()>,
}

impl log::Log for FileLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let tag = match record.level() {
            log::Level::Error => "ERREUR ",
            log::Level::Warn => "",
            _ => "",
        };
        let line = format!("[{}] {}{}\n", chrono::Local::now().format("%H:%M:%S"), tag, record.args());
        if let Ok(mut r) = RECENT.lock() {
            if r.len() >= RECENT_MAX {
                r.pop_front();
            }
            r.push_back(line.trim_end().to_string());
        }
        let _g = self.lock.lock();
        match &self.path {
            None => eprint!("{line}"),
            Some(p) => {
                if fs::metadata(p).map(|m| m.len() > LOG_MAX).unwrap_or(false) {
                    let _ = fs::rename(p, p.with_extension("log.old"));
                }
                if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(p) {
                    let _ = f.write_all(line.as_bytes());
                }
            }
        }
    }
    fn flush(&self) {}
}

/// `path = None` : journal dans la console (développement).
pub fn init_log(path: Option<PathBuf>) {
    let logger = Box::leak(Box::new(FileLogger { path: path.clone(), lock: Mutex::new(()) }));
    let _ = log::set_logger(logger);
    log::set_max_level(if std::env::var_os("VO_DEBUG").is_some() { log::LevelFilter::Debug } else { log::LevelFilter::Info });
    if path.is_some() {
        std::panic::set_hook(Box::new(|info| log::error!("{info}")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_replaces_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        write_atomic(&p, b"one").unwrap();
        write_atomic(&p, b"two").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "two");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "pas de fichier temporaire oublié");
    }

    #[test]
    fn truncate_counts_chars() {
        assert_eq!(truncate("éàü", 2), "éà");
    }
}
