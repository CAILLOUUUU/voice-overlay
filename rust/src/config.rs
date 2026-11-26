//! Dossier des données, identifiants Discord et stockage protégé des secrets.

use crate::util::{read_json, write_json_atomic};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

pub struct Paths {
    pub dir: PathBuf,
    pub config: PathBuf,
    pub token: PathBuf,
    pub settings: PathBuf,
    pub log: PathBuf,
    pub fonts: PathBuf,
}

const DATA_FILES: [&str; 5] = ["config.json", "token.json", "settings.json", "polices", "voice-overlay.log"];

impl Paths {
    /// %APPDATA%\VoiceOverlayCaillou (Windows), ou VO_DATA_DIR si défini.
    pub fn pick() -> Paths {
        let dir = std::env::var_os("VO_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| {
            let base = std::env::var_os("APPDATA").map(PathBuf::from).or_else(config_home).unwrap_or_else(|| PathBuf::from("."));
            base.join("VoiceOverlayCaillou")
        });
        let _ = fs::create_dir_all(&dir);
        // Versions 1.0.0 et avant : les données étaient à côté du .exe. On les déplace une fois.
        if let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
            if exe_dir != dir {
                move_old_data(&exe_dir, &dir);
            }
        }
        Paths::in_dir(dir)
    }

    pub fn in_dir(dir: PathBuf) -> Paths {
        Paths {
            config: dir.join("config.json"),
            token: dir.join("token.json"),
            settings: dir.join("settings.json"),
            log: dir.join("voice-overlay.log"),
            fonts: dir.join("polices"),
            dir,
        }
    }
}

fn config_home() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    Some(if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"))
    })
}

fn move_old_data(from: &Path, to: &Path) {
    for name in DATA_FILES {
        let (src, dst) = (from.join(name), to.join(name));
        if !src.exists() || dst.exists() {
            continue;
        }
        // rename marche si c'est le même disque ; sinon on laisse les fichiers où ils sont.
        let _ = fs::rename(&src, &dst);
    }
}

pub struct Config {
    pub port: u16,
    pub redirect_uri: String,
    pub client_id: String,
    pub client_secret: String,
    /// Si la liste n'est pas vide, seules ces extensions Chrome peuvent envoyer un lobby DPM.
    pub extension_ids: Vec<String>,
}

impl Config {
    pub fn load(paths: &Paths, port_override: Option<u16>) -> Config {
        let raw = read_json(&paths.config).unwrap_or(json!({}));
        let s = |k: &str| raw.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let mut c = Config {
            port: raw.get("port").and_then(Value::as_u64).and_then(|p| u16::try_from(p).ok()).filter(|p| *p > 0).unwrap_or(47474),
            redirect_uri: Some(s("redirectUri")).filter(|x| !x.is_empty()).unwrap_or_else(|| "http://localhost".into()),
            client_id: s("clientId"),
            client_secret: s("clientSecret"),
            extension_ids: raw
                .get("extensionIds")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
                .unwrap_or_default(),
        };
        if let Some(p) = port_override {
            c.port = p;
        }
        if c.client_id.starts_with("COLLE_") {
            c.client_id.clear();
        }
        if c.client_secret.starts_with("COLLE_") {
            c.client_secret.clear();
        }
        // Le secret est rangé dans le coffre de Windows ; s'il est encore dans config.json, on l'y déplace.
        if c.client_secret.is_empty() {
            c.client_secret = secret_get("client-secret").unwrap_or_default();
        } else if secret_set("client-secret", &c.client_secret) {
            c.save(paths);
        }
        c
    }

    pub fn has_credentials(&self) -> bool {
        !self.client_id.is_empty() && !self.client_secret.is_empty()
    }

    pub fn save(&self, paths: &Paths) {
        let in_vault = secret_set("client-secret", &self.client_secret);
        let mut v = json!({
            "port": self.port,
            "redirectUri": self.redirect_uri,
            "clientId": self.client_id,
            "clientSecret": if in_vault { "" } else { self.client_secret.as_str() },
        });
        if !self.extension_ids.is_empty() {
            v["extensionIds"] = json!(self.extension_ids);
        }
        if let Err(e) = write_json_atomic(&paths.config, &v) {
            log::error!("Impossible d'enregistrer config.json : {e}");
        }
    }
}

// ---------- Coffre à secrets (Gestionnaire d'identifiants Windows / Trousseau macOS) ----------
// Repli sur des fichiers si le coffre n'est pas disponible (ou VO_NO_KEYRING=1).

const SERVICE: &str = "VoiceOverlayCaillou";

fn vault_enabled() -> bool {
    std::env::var_os("VO_NO_KEYRING").is_none() && cfg!(any(windows, target_os = "macos"))
}

#[cfg(any(windows, target_os = "macos"))]
fn entry(name: &str) -> Option<keyring::Entry> {
    keyring::Entry::new(SERVICE, name).ok()
}

pub fn secret_get(name: &str) -> Option<String> {
    if !vault_enabled() {
        return None;
    }
    #[cfg(any(windows, target_os = "macos"))]
    {
        return entry(name)?.get_password().ok().filter(|s| !s.is_empty());
    }
    #[allow(unreachable_code)]
    None
}

/// `true` si le secret est bien dans le coffre.
pub fn secret_set(name: &str, value: &str) -> bool {
    if !vault_enabled() {
        return false;
    }
    #[cfg(any(windows, target_os = "macos"))]
    {
        let Some(e) = entry(name) else { return false };
        if value.is_empty() {
            let _ = e.delete_credential();
            return true;
        }
        return e.set_password(value).is_ok();
    }
    #[allow(unreachable_code)]
    {
        let _ = (name, value);
        false
    }
}

/// Jeton Discord : coffre si possible, sinon token.json.
pub fn token_load(paths: &Paths) -> Option<Value> {
    if let Some(v) = secret_get("discord-token").and_then(|s| serde_json::from_str(&s).ok()) {
        return Some(v);
    }
    let v = read_json(&paths.token)?;
    // Ancien token.json en clair : on le range dans le coffre puis on supprime le fichier.
    if secret_set("discord-token", &v.to_string()) {
        let _ = fs::remove_file(&paths.token);
    }
    Some(v)
}

pub fn token_save(paths: &Paths, v: &Value) {
    if secret_set("discord-token", &v.to_string()) {
        let _ = fs::remove_file(&paths.token);
    } else if let Err(e) = write_json_atomic(&paths.token, v) {
        log::error!("Impossible d'enregistrer token.json : {e}");
    }
}

pub fn token_clear(paths: &Paths) {
    secret_set("discord-token", "");
    let _ = fs::remove_file(&paths.token);
}
