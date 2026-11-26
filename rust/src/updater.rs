//! Mise à jour en un clic : on télécharge le .exe de la release GitHub, on vérifie son empreinte
//! SHA-256 (donnée par GitHub), puis on remplace le programme et on le relance.
//!
//! Sous Windows, un .exe en cours d'exécution ne peut pas être écrasé, mais il peut être renommé :
//! l'ancien devient « .old.exe », le nouveau prend sa place, et l'ancien est supprimé au lancement suivant.
//!
//! VO_UPDATE_FEED=http://… : autre adresse que l'API GitHub (seulement pour les tests).

use crate::state::{App, Update, REPO, VERSION};
use crate::util::{m, M};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const MAX_SIZE: usize = 60_000_000;

/// Release trouvée, avec le fichier à télécharger.
#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

fn feed() -> (String, String) {
    match std::env::var("VO_UPDATE_FEED") {
        // Test : le .exe doit venir du même serveur que la liste des releases.
        Ok(f) if !f.is_empty() => {
            let origin = f.splitn(4, '/').take(3).collect::<Vec<_>>().join("/") + "/";
            (f, origin)
        }
        _ => (
            format!("https://api.github.com/repos/{REPO}/releases/latest"),
            format!("https://github.com/{REPO}/releases/download/"),
        ),
    }
}

/// Peut-on installer une mise à jour soi-même ? (le .exe Windows, ou un test)
pub fn can_install() -> bool {
    cfg!(windows) || std::env::var_os("VO_UPDATE_FEED").is_some()
}

pub fn newer(a: &str, b: &str) -> bool {
    let p = |s: &str| -> Vec<u64> { s.split('.').map(|x| x.parse().unwrap_or(0)).collect() };
    let (pa, pb) = (p(a), p(b));
    for i in 0..3 {
        let (x, y) = (pa.get(i).copied().unwrap_or(0), pb.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

/// Lit la réponse « dernière release » de GitHub. Renvoie la version, la page et le .exe à télécharger.
pub fn parse_release(j: &Value, allowed_prefix: &str) -> Option<(String, String, Option<Asset>)> {
    let latest = j["tag_name"].as_str()?.trim_start_matches('v').to_string();
    let valid = latest.split('.').count() <= 3 && latest.split('.').all(|x| !x.is_empty() && x.bytes().all(|b| b.is_ascii_digit()));
    if !valid {
        return None;
    }
    let page = j["html_url"].as_str().unwrap_or("").to_string();
    let wanted = format!("Voice-Overlay-{latest}.exe");
    let asset = j["assets"].as_array().into_iter().flatten().find_map(|a| {
        let url = a["browser_download_url"].as_str()?;
        // Empreinte calculée par GitHub : sans elle, pas d'installation automatique (le lien reste là).
        let sha = a["digest"].as_str()?.strip_prefix("sha256:")?.to_lowercase();
        let ok = a["name"].as_str()? == wanted
            && url.starts_with(allowed_prefix)
            && sha.len() == 64
            && sha.bytes().all(|b| b.is_ascii_hexdigit());
        ok.then(|| Asset { url: url.to_string(), sha256: sha, size: a["size"].as_u64().unwrap_or(0) })
    });
    Some((latest, page, asset))
}

pub async fn check(app: &App) {
    let (url, prefix) = feed();
    let Ok(r) = reqwest::Client::new()
        .get(url)
        .header("User-Agent", "voice-overlay")
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(8))
        .send()
        .await
    else {
        return;
    };
    if !r.status().is_success() {
        return;
    }
    let Ok(j) = r.json::<Value>().await else { return };
    let Some((latest, page, asset)) = parse_release(&j, &prefix) else { return };
    let page_ok = page.starts_with(&format!("https://github.com/{REPO}/")) || std::env::var_os("VO_UPDATE_FEED").is_some();
    if !newer(&latest, VERSION) || !page_ok {
        return;
    }
    let can = can_install() && asset.is_some();
    app.update(|i| {
        if i.update.as_ref().is_some_and(|u| u.version == latest) {
            return;
        }
        log::info!("Nouvelle version disponible : {latest}");
        i.update = Some(Update { version: latest, url: page, can_install: can });
        i.update_asset = asset;
        i.broadcast();
    });
}

pub async fn check_loop(app: Arc<App>) {
    tokio::time::sleep(Duration::from_secs(4)).await;
    loop {
        check(&app).await;
        tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn set_status(app: &App, status: &'static str, error: Option<M>) {
    app.update(|i| {
        i.update_status = Some((status, error));
        i.broadcast();
    });
}

/// Téléchargement + vérification + remplacement + relance. Appelé par « Installer ».
pub async fn install(app: Arc<App>) {
    let Some(asset) = app.read(|i| i.update_asset.clone()) else {
        return set_status(&app, "error", Some(m("Aucune mise à jour à installer.", "No update to install.")));
    };
    if app.read(|i| matches!(i.update_status, Some(("downloading" | "restarting", _)))) {
        return;
    }
    set_status(&app, "downloading", None);
    log::info!("Mise à jour : téléchargement de {}", asset.url);
    let data = match download(&asset).await {
        Ok(d) => d,
        Err(e) => {
            log::error!("Mise à jour : {}", e.fr);
            return set_status(&app, "error", Some(e));
        }
    };
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return set_status(&app, "error", Some(m(e.to_string(), e.to_string()))),
    };
    if let Err(e) = replace_exe(&exe, &data) {
        log::error!("Mise à jour : remplacement impossible : {e}");
        return set_status(&app, "error", Some(m(format!("remplacement impossible ({e})"), format!("could not replace the program ({e})"))));
    }
    set_status(&app, "restarting", None);
    log::info!("Mise à jour installée : redémarrage.");
    // Le temps que la page reçoive « redémarrage », puis on passe la main au nouveau programme.
    tokio::time::sleep(Duration::from_millis(400)).await;
    app.save_store();
    let mut args: Vec<String> = std::env::args().skip(1).filter(|a| a != "--open" && a != "--after-update").collect();
    if !args.iter().any(|a| a == "--silent") {
        args.push("--silent".into());
    }
    args.push("--after-update".into());
    match std::process::Command::new(&exe).args(&args).spawn() {
        Ok(_) => {
            tokio::task::spawn_blocking(crate::platform::exit_app);
        }
        Err(e) => {
            // Le nouveau programme ne démarre pas : on remet l'ancien.
            let _ = std::fs::rename(&exe, exe.with_extension("broken.exe"));
            let _ = std::fs::rename(old_path(&exe), &exe);
            log::error!("Mise à jour : lancement impossible ({e}), ancienne version remise.");
            set_status(&app, "error", Some(m(format!("lancement impossible ({e})"), format!("could not start the new version ({e})"))));
        }
    }
}

async fn download(asset: &Asset) -> Result<Vec<u8>, M> {
    let r = reqwest::Client::new()
        .get(&asset.url)
        .header("User-Agent", "voice-overlay")
        .timeout(Duration::from_secs(300))
        .send()
        .await
        .map_err(|e| m(format!("téléchargement impossible ({e})"), format!("download failed ({e})")))?;
    if !r.status().is_success() {
        let c = r.status().as_u16();
        return Err(m(format!("téléchargement impossible (HTTP {c})"), format!("download failed (HTTP {c})")));
    }
    let data = r.bytes().await.map_err(|e| m(format!("téléchargement interrompu ({e})"), format!("download interrupted ({e})")))?;
    if data.len() > MAX_SIZE || (asset.size > 0 && data.len() as u64 != asset.size) {
        return Err(m("le fichier téléchargé n'a pas la taille attendue", "the downloaded file has the wrong size"));
    }
    let got = hex(&Sha256::digest(&data));
    if got != asset.sha256 {
        return Err(m(
            "le fichier téléchargé ne correspond pas à l'empreinte de GitHub, installation annulée",
            "the downloaded file doesn't match GitHub's fingerprint, installation cancelled",
        ));
    }
    Ok(data.to_vec())
}

fn old_path(exe: &Path) -> PathBuf {
    exe.with_extension("old.exe")
}

/// Écrit le nouveau programme à côté, puis échange les deux par renommage.
pub fn replace_exe(exe: &Path, data: &[u8]) -> std::io::Result<()> {
    let new = exe.with_extension("new.exe");
    let old = old_path(exe);
    std::fs::write(&new, data)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755))?;
    }
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old)?;
    if let Err(e) = std::fs::rename(&new, exe) {
        let _ = std::fs::rename(&old, exe);
        return Err(e);
    }
    Ok(())
}

/// Au démarrage : supprime l'ancienne version laissée par une mise à jour.
pub fn cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        let old = old_path(&exe);
        if old.exists() {
            // Juste après une mise à jour, l'ancien programme peut encore être en train de se fermer.
            for _ in 0..20 {
                if std::fs::remove_file(&old).is_ok() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        }
        let _ = std::fs::remove_file(exe.with_extension("new.exe"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PREFIX: &str = "https://github.com/CAILLOUUUU/voice-overlay/releases/download/";

    fn release(name: &str, url: &str, digest: Option<&str>) -> Value {
        json!({ "tag_name": "v2.0.0", "html_url": "https://github.com/CAILLOUUUU/voice-overlay/releases/tag/v2.0.0",
                "assets": [{ "name": name, "browser_download_url": url, "size": 3, "digest": digest }] })
    }

    #[test]
    fn finds_exe_with_digest() {
        let sha = "a".repeat(64);
        let j = release("Voice-Overlay-2.0.0.exe", &format!("{PREFIX}v2.0.0/Voice-Overlay-2.0.0.exe"), Some(&format!("sha256:{sha}")));
        let (v, _, a) = parse_release(&j, PREFIX).unwrap();
        assert_eq!(v, "2.0.0");
        assert_eq!(a.unwrap().sha256, sha);
    }

    #[test]
    fn refuses_unsafe_assets() {
        let sha = format!("sha256:{}", "a".repeat(64));
        let other_host = release("Voice-Overlay-2.0.0.exe", "https://evil.com/Voice-Overlay-2.0.0.exe", Some(&sha));
        assert!(parse_release(&other_host, PREFIX).unwrap().2.is_none());
        let no_digest = release("Voice-Overlay-2.0.0.exe", &format!("{PREFIX}x.exe"), None);
        assert!(parse_release(&no_digest, PREFIX).unwrap().2.is_none());
        let wrong_name = release("installer.exe", &format!("{PREFIX}x.exe"), Some(&sha));
        assert!(parse_release(&wrong_name, PREFIX).unwrap().2.is_none());
    }

    #[test]
    fn replace_swaps_files() {
        let d = tempfile::tempdir().unwrap();
        let exe = d.path().join("Voice-Overlay.exe");
        std::fs::write(&exe, b"old").unwrap();
        replace_exe(&exe, b"new").unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        assert_eq!(std::fs::read(old_path(&exe)).unwrap(), b"old");
    }

    #[test]
    fn version_compare() {
        assert!(newer("1.10.0", "1.9.9"));
        assert!(!newer("1.2.0", "1.2.0"));
        assert!(newer("2", "1.9"));
    }
}
