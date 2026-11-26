//! VOICE OVERLAY - CAILLOU FEAT CLAUDE
//! Suit automatiquement ton salon vocal Discord et affiche qui est dedans dans une source navigateur OBS.
//!
//!   voice-overlay            -> mode normal (Discord desktop doit être ouvert), ouvre les réglages
//!   voice-overlay --silent   -> sans ouvrir les réglages (lancement avec Windows)
//!   voice-overlay --demo     -> faux membres (sans Discord)
//!   voice-overlay --port=N   -> autre port que 47474 (ou VO_PORT=N)

// Pas de fenêtre console pour le .exe de production.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod assets;
mod config;
mod discord;
mod dpm;
mod league;
mod platform;
mod server;
mod settings;
mod similarity;
mod state;
mod util;

use config::{Config, Paths};
use state::{App, Inner, Update, APP_NAME, REPO, VERSION};
use std::sync::Arc;
use std::time::Duration;

struct Args {
    demo: bool,
    silent: bool,
    open: bool,
    port: Option<u16>,
}

fn args() -> Args {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| a.iter().any(|x| x == f);
    let port = a
        .iter()
        .find_map(|x| x.strip_prefix("--port=").map(String::from))
        .or_else(|| std::env::var("VO_PORT").ok())
        .and_then(|p| p.parse().ok())
        .filter(|p: &u16| *p > 0);
    Args { demo: has("--demo"), silent: has("--silent"), open: has("--open"), port }
}

fn ui_is_fr(lang: &str) -> bool {
    match lang {
        "fr" => true,
        "en" => false,
        _ => sys_locale::get_locale().unwrap_or_default().to_lowercase().starts_with("fr"),
    }
}

// ---------- Nouvelle version disponible ? (question posée à GitHub, rien d'autre n'est envoyé) ----------
fn newer(a: &str, b: &str) -> bool {
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

async fn check_update(app: &App) {
    let r = reqwest::Client::new()
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("User-Agent", "voice-overlay")
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(8))
        .send()
        .await;
    let Ok(r) = r else { return };
    if !r.status().is_success() {
        return;
    }
    let Ok(j) = r.json::<serde_json::Value>().await else { return };
    let latest = j["tag_name"].as_str().unwrap_or("").trim_start_matches('v').to_string();
    let url = j["html_url"].as_str().unwrap_or("").to_string();
    let valid = !latest.is_empty() && latest.split('.').count() <= 3 && latest.split('.').all(|x| !x.is_empty() && x.bytes().all(|b| b.is_ascii_digit()));
    if valid && newer(&latest, VERSION) && url.starts_with(&format!("https://github.com/{REPO}/")) {
        app.update(|i| {
            i.update = Some(Update { version: latest, url });
            i.broadcast();
        });
    }
}

async fn update_loop(app: Arc<App>) {
    tokio::time::sleep(Duration::from_secs(4)).await;
    loop {
        check_update(&app).await;
        tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
    }
}

// ---------- Mode démo ----------
async fn run_demo(app: Arc<App>) {
    let names = ["CAILLOU", "Zed main", "Jungle Diff", "mid or feed", "Support Gap", "ADC Enjoyer", "Toplaner"];
    let add = |i: &mut Inner, n: usize| {
        let id = (100_000_000_000_000_000u64 + n as u64 * 4_194_304 * 7).to_string();
        i.upsert_member(state::Member {
            id: id.clone(),
            name: names[n % names.len()].into(),
            avatar: format!("https://cdn.discordapp.com/embed/avatars/{}.png", n % 6),
            deco: None,
            mute: false,
            deaf: false,
        });
        id
    };
    app.update(|i| {
        i.channel = Some(state::Channel { id: "1".into(), name: "Inhouse #12 — Équipe Bleue".into(), guild_id: Some("1".into()) });
        let ids: Vec<String> = (0..5).map(|n| add(i, n)).collect();
        i.self_id = Some(ids[0].clone());
        i.broadcast();
    });
    log::info!("Mode démo : faux membres actifs.");
    // Petit générateur pseudo-aléatoire : pas besoin d'une bibliothèque pour une démo.
    let mut seed = util::now_ms() as u64 | 1;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % 10_000) as f64 / 10_000.0
    };
    let mut next = 5;
    let mut tick = 0u32;
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        tick += 1;
        let rolls: Vec<f64> = (0..10).map(|_| rnd()).collect();
        let r = rnd();
        let pick = rnd();
        app.update(|i| {
            let ids: Vec<String> = i.members.iter().map(|m| m.id.clone()).collect();
            for (k, id) in ids.iter().enumerate() {
                if rolls[k % rolls.len()] < 0.25 && !i.speaking.remove(id) {
                    i.speaking.insert(id.clone());
                }
            }
            i.broadcast_speaking();
            if !tick.is_multiple_of(8) {
                return;
            }
            if r < 0.35 && ids.len() > 3 {
                let id = &ids[1 + (pick * (ids.len() - 1) as f64) as usize];
                i.remove_member(id);
                i.prune_people();
            } else if r < 0.7 && ids.len() < 7 {
                add(i, next);
                next += 1;
            } else if let Some(m) = i.members.get_mut((pick * ids.len() as f64) as usize) {
                m.mute = !m.mute;
            }
            i.broadcast();
        });
    }
}

fn main() {
    let args = args();
    let paths = Paths::pick();
    // Développement (cargo run) : journal dans la console ; .exe de production : dans un fichier.
    util::init_log(if cfg!(debug_assertions) { None } else { Some(paths.log.clone()) });
    let config = Config::load(&paths, args.port);
    let port = config.port;
    let loaded = settings::load(util::read_json(&paths.settings));
    let inner = Inner::new(loaded, args.demo);
    let lang = inner.lang.clone();
    let app = App::new(inner, config, paths);
    let base = format!("http://localhost:{port}");

    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("tokio");
    // Une seule instance : si le port est déjà pris, l'overlay tourne déjà, on ouvre ses réglages.
    let listener = match rt.block_on(tokio::net::TcpListener::bind(("127.0.0.1", port))) {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            log::error!("Le port {port} est déjà utilisé : l'overlay tourne déjà.");
            if !args.silent {
                platform::open_url(&format!("{base}/settings"));
            }
            return;
        }
        Err(e) => {
            log::error!("{e}");
            std::process::exit(1);
        }
    };
    log::info!("=== {APP_NAME} {VERSION} ===");
    log::info!("Réglages : {base}/settings");
    log::info!("Overlay  : {base}/   (à mettre dans une source navigateur OBS)");

    let a = app.clone();
    rt.spawn(async move {
        if let Err(e) = axum::serve(listener, server::router(a)).await {
            log::error!("Serveur : {e}");
        }
    });
    rt.spawn(app.clone().broadcaster());
    rt.spawn(app.clone().people_saver());
    rt.spawn(app.clone().pruner());
    rt.spawn(update_loop(app.clone()));
    if args.demo {
        rt.spawn(run_demo(app.clone()));
    } else {
        rt.spawn(discord::run(app.clone()));
        if cfg!(windows) || std::env::var_os("VO_LEAGUE").is_some() {
            rt.spawn(league::run(app.clone()));
        }
    }
    if args.open || (!args.silent && !cfg!(debug_assertions)) {
        platform::open_url(&format!("{base}/settings"));
    }

    #[cfg(windows)]
    {
        let a = app.clone();
        // L'icône vit sur le fil principal ; « Quitter » enregistre puis ferme l'appli.
        platform::run_tray(format!("{base}/settings"), ui_is_fr(&lang), move || {
            a.save_store();
            std::process::exit(0);
        });
    }
    #[cfg(not(windows))]
    {
        let _ = ui_is_fr(&lang);
        let a = app.clone();
        rt.block_on(async move {
            let _ = tokio::signal::ctrl_c().await;
            a.save_store();
            std::process::exit(0);
        });
    }
    // Icône indisponible : on continue sans elle.
    rt.block_on(std::future::pending::<()>());
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_compare() {
        assert!(super::newer("1.10.0", "1.9.9"));
        assert!(!super::newer("1.2.0", "1.2.0"));
        assert!(super::newer("2", "1.9"));
    }
}
