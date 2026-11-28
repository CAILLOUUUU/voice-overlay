//! Serveur HTTP local : pages, polices, API des réglages et flux temps réel (SSE).

use crate::discord::credentials_changed;
use crate::platform;
use crate::settings::{self, clean_name, clean_settings, Profile, FORGET_CHOICES};
use crate::state::{App, Push, APP_NAME, VERSION};
use crate::util::{m, now_ms, truncate, M};
use axum::body::{to_bytes, Body};
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Router;
use futures_util::stream::{self, Stream, StreamExt};
use serde_json::{json, Value};
use std::convert::Infallible;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;

// ---------- Polices fournies + fichiers ajoutés par l'utilisateur ----------
/// Polices libres (licence OFL) embarquées dans l'appli : [nom, graisses disponibles]
pub const BUILTIN_FONTS: [(&str, &[u16]); 12] = [
    ("Inter", &[500, 700]),
    ("Poppins", &[500, 700]),
    ("Montserrat", &[500, 700]),
    ("Rubik", &[500, 700]),
    ("Fredoka", &[500, 700]),
    ("Oswald", &[500, 700]),
    ("Barlow Condensed", &[500, 700]),
    ("Rajdhani", &[500, 700]),
    ("Orbitron", &[500, 700]),
    ("Bebas Neue", &[400]),
    ("Russo One", &[400]),
    ("Bangers", &[400]),
];
const FONT_EXT: [&str; 4] = [".ttf", ".otf", ".woff", ".woff2"];
const FONT_MAX: usize = 10_000_000;

fn ext_of(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 => name[i..].to_lowercase(),
        _ => String::new(),
    }
}

/// Nom de fichier sûr (pas de sous-dossier, extension autorisée), sinon None.
pub fn safe_font_name(raw: &str) -> Option<String> {
    let raw = raw.replace('\\', "/");
    let base = raw.rsplit('/').next().unwrap_or("");
    let cleaned: String = base
        .chars()
        .map(|c| if c.is_alphanumeric() && c.is_ascii() || matches!(c, '_' | ' ' | '.' | '(' | ')' | '-') { c } else { '_' })
        .take(80)
        .collect();
    let ext = ext_of(&cleaned);
    let stem = &cleaned[..cleaned.len() - ext.len()];
    (FONT_EXT.contains(&ext.as_str()) && !stem.trim().is_empty() && !stem.starts_with('.')).then_some(cleaned)
}

fn font_family(file: &str) -> String {
    let ext = ext_of(file);
    file[..file.len() - ext.len()].to_string()
}

fn list_fonts(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|f| FONT_EXT.contains(&ext_of(f).as_str())).collect())
        .unwrap_or_default();
    v.sort();
    v
}

fn slug(n: &str) -> String {
    n.to_lowercase().replace(' ', "-")
}

fn encode_uri_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn decode_uri_component(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let h = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(h, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

pub fn fonts_css(dir: &Path) -> String {
    let mut css = String::new();
    for (name, weights) in BUILTIN_FONTS {
        for w in weights {
            let range = if weights.len() == 1 { "100 900".to_string() } else { w.to_string() };
            css += &format!("@font-face{{font-family:\"{name}\";font-weight:{range};font-display:swap;src:url(\"/webfonts/{}-{w}.woff2\") format(\"woff2\")}}\n", slug(name));
        }
    }
    for f in list_fonts(dir) {
        css += &format!("@font-face{{font-family:{};font-weight:100 900;font-display:swap;src:url(\"/polices/{}\")}}\n", json!(font_family(&f)), encode_uri_component(&f));
    }
    css
}

// ---------- Réponses ----------

fn json_resp(code: u16, v: Value) -> Response {
    (StatusCode::from_u16(code).unwrap(), [(header::CONTENT_TYPE, "application/json; charset=utf-8"), (header::CACHE_CONTROL, "no-cache")], v.to_string()).into_response()
}
fn err(code: u16, msg: M) -> Response {
    json_resp(code, json!({ "error": msg }))
}
fn plain(code: u16, text: &str) -> Response {
    json_resp(code, json!({ "error": text }))
}
fn empty(code: u16) -> Response {
    StatusCode::from_u16(code).unwrap().into_response()
}
fn ok() -> Response {
    json_resp(200, json!({ "ok": true }))
}

fn asset(name: &str, ctype: &'static str) -> Response {
    match crate::assets::get(name) {
        Some(b) => ([(header::CONTENT_TYPE, ctype), (header::CACHE_CONTROL, "no-cache")], b.into_owned()).into_response(),
        None => empty(404),
    }
}

// ---------- Contrôles d'accès ----------

fn host_ok(h: &HeaderMap, port: u16) -> bool {
    let host = h.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("");
    host == format!("localhost:{port}") || host == format!("127.0.0.1:{port}")
}

/// Seules les pages servies par ce serveur ont le droit de modifier les réglages.
fn is_local(h: &HeaderMap, port: u16) -> bool {
    if !host_ok(h, port) {
        return false;
    }
    match h.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        None => true,
        Some(o) => o == format!("http://localhost:{port}") || o == format!("http://127.0.0.1:{port}"),
    }
}

/// L'extension Chrome (pont DPM) n'a le droit d'appeler que /api/dpm.
fn is_extension(h: &HeaderMap, port: u16, allowed: &[String]) -> bool {
    let origin = h.get(header::ORIGIN).and_then(|v| v.to_str().ok()).unwrap_or("");
    let Some(id) = origin.strip_prefix("chrome-extension://") else { return false };
    host_ok(h, port) && id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b)) && (allowed.is_empty() || allowed.iter().any(|a| a == id))
}

// ---------- Flux temps réel ----------

fn events(app: Arc<App>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let first = Event::default().data(app.snapshot_string());
    let rx = BroadcastStream::new(app.tx.subscribe());
    let rest = rx.map(move |m| {
        Ok(match m {
            Ok(Push::Full(s)) => Event::default().data(s.as_str()),
            Ok(Push::Speaking(s)) => Event::default().event("speaking").data(s.as_str()),
            // Page trop lente : on lui renvoie tout l'état d'un coup.
            Err(_) => Event::default().data(app.snapshot_string()),
        })
    });
    Sse::new(stream::once(async move { Ok(first) }).chain(rest)).keep_alive(KeepAlive::new().interval(Duration::from_secs(20)).text("ping"))
}

// ---------- Routage ----------

pub fn router(app: Arc<App>) -> Router {
    Router::new().fallback(handle).with_state(app)
}

async fn handle(State(app): State<Arc<App>>, req: Request) -> Response {
    let port = app.config.lock().unwrap().port;
    let headers = req.headers().clone();
    // Protection contre le « DNS rebinding » : un site web ne doit pas pouvoir lire l'overlay.
    if !host_ok(&headers, port) {
        return empty(403);
    }
    let path = req.uri().path().to_string();
    let method = req.method().clone();
    match path.as_str() {
        "/events" => return events(app).into_response(),
        "/" => return asset("overlay.html", "text/html; charset=utf-8"),
        "/settings" => return asset("settings.html", "text/html; charset=utf-8"),
        "/icon.png" | "/favicon.ico" => return asset("icon.png", "image/png"),
        "/licences" => return asset("LICENCES-TIERS.txt", "text/plain; charset=utf-8"),
        "/fonts.css" => return ([(header::CONTENT_TYPE, "text/css; charset=utf-8"), (header::CACHE_CONTROL, "no-cache")], fonts_css(&app.paths.fonts)).into_response(),
        "/api/dpm" | "/api/dpm/ping" => return dpm_route(&app, req, port).await,
        _ => {}
    }
    if let Some(f) = path.strip_prefix("/webfonts/") {
        if f.ends_with(".woff2") && f[..f.len() - 6].bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') && f.len() > 6 {
            return asset(&format!("webfonts/{f}"), "font/woff2");
        }
        return empty(404);
    }
    if let Some(f) = path.strip_prefix("/polices/") {
        let Some(name) = decode_uri_component(f).and_then(|n| safe_font_name(&n)) else { return empty(404) };
        return match tokio::fs::read(app.paths.fonts.join(&name)).await {
            Ok(data) => {
                let ct = match ext_of(&name).as_str() {
                    ".ttf" => "font/ttf",
                    ".otf" => "font/otf",
                    ".woff" => "font/woff",
                    _ => "font/woff2",
                };
                ([(header::CONTENT_TYPE, ct), (header::CACHE_CONTROL, "no-cache")], data).into_response()
            }
            Err(_) => empty(404),
        };
    }
    if path.starts_with("/api/") {
        if !is_local(&headers, port) {
            return plain(403, "interdit");
        }
        return api(&app, &path, method, req).await;
    }
    empty(404)
}

async fn dpm_route(app: &Arc<App>, req: Request, port: u16) -> Response {
    let allowed = app.config.lock().unwrap().extension_ids.clone();
    let h = req.headers().clone();
    // Une extension Chrome peut appeler ces deux adresses sans permission spéciale :
    // l'appli autorise les origines chrome-extension:// (CORS), et seulement elles.
    let ext = is_extension(&h, port, &allowed);
    let mut r = if req.method() == Method::OPTIONS {
        empty(if ext { 204 } else { 403 })
    } else if !ext && !is_local(&h, port) {
        plain(403, "interdit")
    } else if req.uri().path() == "/api/dpm/ping" {
        // « L'appli est-elle lancée ? » : pour que l'extension affiche un état ou un lien de téléchargement.
        json_resp(200, json!({ "app": "voice-overlay", "version": VERSION }))
    } else if req.method() != Method::POST {
        plain(405, "POST attendu")
    } else {
        match to_bytes(req.into_body(), 200_000).await.ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) {
            Some(b) => {
                app.update(|i| i.dpm_update(&b));
                ok()
            }
            None => plain(400, "JSON invalide"),
        }
    };
    if ext {
        let hd = r.headers_mut();
        if let Some(o) = h.get(header::ORIGIN) {
            hd.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, o.clone());
        }
        hd.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("GET, POST, OPTIONS"));
        hd.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, HeaderValue::from_static("Content-Type"));
        hd.insert("Access-Control-Allow-Private-Network", HeaderValue::from_static("true"));
        hd.insert(header::VARY, HeaderValue::from_static("Origin"));
    }
    r
}

fn query(req: &Request, key: &str) -> Option<String> {
    req.uri().query()?.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        (k == key).then(|| decode_uri_component(&v.replace('+', " ")).unwrap_or_default())
    })
}

fn s(b: &Value, k: &str) -> String {
    match b.get(k) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(v) => v.to_string(),
    }
}

async fn api(app: &Arc<App>, path: &str, method: Method, req: Request) -> Response {
    let post = method == Method::POST;
    if !post {
        return match path {
            "/api/state" => {
                let (lang, profiles, forget) = app.read(|i| (i.lang.clone(), i.profiles.clone(), i.forget_after));
                let (client_id, has, port) = {
                    let c = app.config.lock().unwrap();
                    (c.client_id.clone(), c.has_credentials(), c.port)
                };
                let autostart = platform::CAN_AUTOSTART && tokio::task::spawn_blocking(platform::autostart).await.unwrap_or(false);
                json_resp(200, json!({
                    "app": APP_NAME, "version": VERSION, "lang": lang, "profiles": profiles, "forgetAfter": forget,
                    "defaults": settings::defaults(), "clientId": client_id, "hasConfig": has, "port": port,
                    "canAutostart": platform::CAN_AUTOSTART, "autostart": autostart, "isExe": true,
                    "dataDir": app.paths.dir.display().to_string(),
                    // Ce que cette version sait faire (la page cache le reste).
                    "features": ["diagnostics", "update-install"],
                }))
            }
            "/api/diagnostics" => {
                let text = diagnostics(app).await;
                ([(header::CONTENT_TYPE, "text/plain; charset=utf-8"), (header::CACHE_CONTROL, "no-cache")], text).into_response()
            }
            "/api/files" => json_resp(200, json!({
                "builtinFonts": BUILTIN_FONTS.iter().map(|f| f.0).collect::<Vec<_>>(),
                "fonts": list_fonts(&app.paths.fonts).iter().map(|f| json!({ "file": f, "family": font_family(f) })).collect::<Vec<_>>(),
            })),
            _ => plain(404, "inconnu"),
        };
    }

    // Import d'une police : le fichier est envoyé tel quel.
    if path == "/api/upload" {
        if query(&req, "kind").as_deref() != Some("font") {
            return err(400, m("Type inconnu.", "Unknown type."));
        }
        let Some(name) = query(&req, "name").and_then(|n| safe_font_name(&n)) else {
            let list = FONT_EXT.join(", ");
            return err(400, m(format!("Format non accepté. Formats possibles : {list}"), format!("Unsupported format. Accepted: {list}")));
        };
        let Ok(data) = to_bytes(req.into_body(), FONT_MAX).await else {
            return err(400, m("Fichier trop gros.", "File too large."));
        };
        if data.is_empty() {
            return err(400, m("Fichier vide.", "Empty file."));
        }
        let _ = std::fs::create_dir_all(&app.paths.fonts);
        if let Err(e) = std::fs::write(app.paths.fonts.join(&name), &data) {
            return plain(400, &e.to_string());
        }
        app.update(|i| {
            i.assets_version = now_ms();
            i.broadcast();
        });
        return json_resp(200, json!({ "ok": true, "file": name, "family": font_family(&name) }));
    }

    let body: Body = req.into_body();
    let b: Value = match to_bytes(body, 200_000).await {
        Ok(bytes) if bytes.is_empty() => json!({}),
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => return plain(400, &e.to_string()),
        },
        Err(_) => return plain(400, "trop gros"),
    };

    match path {
        "/api/settings" => {
            let id = s(&b, "profile");
            let found = app.update(|i| {
                let Some(p) = i.profiles.iter_mut().find(|p| p.id == id) else { return false };
                p.settings = clean_settings(b.get("settings"), &p.settings);
                i.broadcast();
                true
            });
            if !found {
                return err(404, m("Profil introuvable.", "Profile not found."));
            }
            app.save_store();
            ok()
        }
        "/api/people-forget" => {
            let n = b.get("minutes").and_then(Value::as_f64).map(|f| f as i64).filter(|n| FORGET_CHOICES.contains(n)).unwrap_or(10);
            app.update(|i| {
                i.forget_after = n;
                i.prune_people();
                i.broadcast();
            });
            app.save_store();
            ok()
        }
        "/api/lang" => {
            let l = s(&b, "lang");
            app.update(|i| {
                i.lang = if l == "fr" || l == "en" { l } else { "auto".into() };
                i.broadcast();
            });
            app.save_store();
            ok()
        }
        // --- Profils : créer (copie d'un profil), renommer, supprimer ---
        "/api/profiles" => {
            let id = s(&b, "id");
            let action = s(&b, "action");
            let r = app.update(|i| -> Result<Value, (u16, M)> {
                if action == "create" {
                    if i.profiles.len() >= 20 {
                        return Err((400, m("20 profils maximum.", "20 profiles maximum.")));
                    }
                    let mut name = clean_name(&s(&b, "name"));
                    if name.is_empty() {
                        name = format!("Profil {}", i.profiles.len() + 1);
                    }
                    let base = i.profile(&id).map(|p| p.settings.clone()).unwrap_or_else(settings::defaults);
                    let np = Profile { id: i.new_profile_id(&name), name, settings: clean_settings(Some(&Value::Object(base)), &settings::defaults()) };
                    i.profiles.push(np.clone());
                    i.broadcast();
                    return Ok(json!({ "ok": true, "profile": np, "profiles": i.profiles }));
                }
                let Some(pos) = i.profiles.iter().position(|p| p.id == id) else { return Err((404, m("Profil introuvable.", "Profile not found."))) };
                match action.as_str() {
                    "rename" => {
                        let n = clean_name(&s(&b, "name"));
                        if !n.is_empty() {
                            i.profiles[pos].name = n;
                        }
                    }
                    "delete" => {
                        if i.profiles.len() == 1 {
                            return Err((400, m("Il faut garder au moins un profil.", "You need to keep at least one profile.")));
                        }
                        i.profiles.remove(pos);
                    }
                    _ => return Err((400, m("Action inconnue.", "Unknown action."))),
                }
                i.broadcast();
                Ok(json!({ "ok": true, "profiles": i.profiles }))
            });
            match r {
                Ok(v) => {
                    app.save_store();
                    json_resp(200, v)
                }
                Err((c, msg)) => err(c, msg),
            }
        }
        "/api/config" => {
            let id = s(&b, "clientId").trim().to_string();
            let mut secret = s(&b, "clientSecret").trim().to_string();
            {
                let c = app.config.lock().unwrap();
                if secret.is_empty() && id == c.client_id {
                    secret = c.client_secret.clone();
                }
            }
            if !((15..=22).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit())) {
                return err(400, m("Le Client ID doit être une suite de chiffres.", "The Client ID must be digits only."));
            }
            if secret.chars().count() < 10 {
                return err(400, m("Le Client Secret a l'air incomplet.", "The Client Secret looks incomplete."));
            }
            {
                let mut c = app.config.lock().unwrap();
                c.client_id = id;
                c.client_secret = secret;
                c.save(&app.paths);
            }
            credentials_changed(app);
            ok()
        }
        // --- Réglages par personne ---
        "/api/person" => {
            let id = s(&b, "id");
            let r = app.update(|i| {
                if !i.has_member(&id) {
                    return false;
                }
                let mut c = i.custom.get(&id).cloned().unwrap_or_default();
                if b.get("name").is_some() {
                    c.name = truncate(s(&b, "name").trim(), 40);
                }
                if let Some(h) = b.get("hidden") {
                    c.hidden = h.as_bool().unwrap_or(!h.is_null());
                }
                if b.get("role").is_some() {
                    c.role = settings::role_or_empty(b.get("role"));
                }
                if c.is_empty() {
                    i.custom.remove(&id);
                } else {
                    i.custom.insert(id.clone(), c);
                }
                i.last_seen.insert(id.clone(), now_ms());
                i.save_people_soon();
                i.broadcast();
                true
            });
            if r { ok() } else { err(404, m("Cette personne n'est plus dans le vocal.", "This person is no longer in the voice channel.")) }
        }
        "/api/order" => {
            let list: Vec<String> = b.get("order").and_then(Value::as_array).map(|a| a.iter().map(|v| s(&json!({ "x": v }), "x")).collect()).unwrap_or_default();
            app.update(|i| {
                // Les personnes absentes gardent leur place, derrière celles qui sont là.
                let present: Vec<String> = list.into_iter().filter(|id| i.has_member(id)).collect();
                let rest: Vec<String> = i.order.iter().filter(|id| !present.contains(id) && !i.has_member(id)).cloned().collect();
                i.order = present.into_iter().chain(rest).collect();
                i.save_people_soon();
                i.broadcast();
            });
            ok()
        }
        "/api/dpm-forget" => {
            let id = s(&b, "id");
            app.update(|i| i.dpm_forget(if id.is_empty() { None } else { Some(&id) }));
            ok()
        }
        "/api/people-reset" => {
            app.update(|i| {
                i.custom.clear();
                i.order.clear();
                i.last_seen.clear();
                i.save_people_soon();
                i.broadcast();
            });
            ok()
        }
        "/api/delete-file" => {
            let Some(name) = (s(&b, "kind") == "font").then(|| safe_font_name(&s(&b, "file"))).flatten() else {
                return err(400, m("Fichier inconnu.", "Unknown file."));
            };
            let _ = std::fs::remove_file(app.paths.fonts.join(name));
            app.update(|i| {
                i.assets_version = now_ms();
                i.broadcast();
            });
            ok()
        }
        "/api/autostart" => {
            if !platform::CAN_AUTOSTART {
                return err(400, m("Disponible uniquement avec le .exe sous Windows.", "Only available with the Windows .exe."));
            }
            let on = b.get("enabled").and_then(Value::as_bool).unwrap_or(false);
            let now = tokio::task::spawn_blocking(move || {
                platform::set_autostart(on);
                platform::autostart()
            })
            .await
            .unwrap_or(false);
            json_resp(200, json!({ "ok": true, "autostart": now }))
        }
        "/api/update-install" => {
            if !app.read(|i| i.update.as_ref().is_some_and(|u| u.can_install)) {
                return err(400, m("Aucune mise à jour à installer.", "No update to install."));
            }
            tokio::spawn(crate::updater::install(app.clone()));
            ok()
        }
        "/api/show-settings" => {
            let port = app.config.lock().unwrap().port;
            platform::show_settings(&format!("http://localhost:{port}/settings"));
            ok()
        }
        "/api/retry" => {
            app.request_restart();
            ok()
        }
        "/api/open-data" => {
            platform::open_folder(&app.paths.dir);
            ok()
        }
        "/api/quit" => {
            log::info!("Fermeture demandée depuis la page de réglages.");
            let a = app.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(300)).await;
                a.save_store();
                tokio::task::spawn_blocking(platform::exit_app);
            });
            ok()
        }
        _ => plain(404, "inconnu"),
    }
}

/// Rapport à copier pour signaler un souci : versions, états, détection League/DPM, fin du journal.
/// Pas de mot de passe, pas de jeton, pas de Client Secret.
async fn diagnostics(app: &Arc<App>) -> String {
    let (port, has_cfg, ext) = {
        let c = app.config.lock().unwrap();
        (c.port, c.has_credentials(), c.extension_ids.len())
    };
    let autostart = platform::CAN_AUTOSTART && tokio::task::spawn_blocking(platform::autostart).await.unwrap_or(false);
    let snap = app.read(|i| i.snapshot());
    let league_note = app.read(|i| i.league.note.clone());
    let mut sys = sysinfo::System::new();
    let pid = sysinfo::get_current_pid().ok();
    if let Some(pid) = pid {
        sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
    }
    let mem = pid.and_then(|p| sys.process(p)).map(|p| p.memory() / 1_048_576).unwrap_or(0);
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_default();
    let hide_home = |s: String| if home.is_empty() { s } else { s.replace(&home, "~") };
    let msg_en = snap["statusMsg"]["en"].as_str().unwrap_or("").to_string();
    let mut out = vec![
        format!("Voice Overlay {VERSION} (Rust) - diagnostic du {}", chrono::Local::now().format("%Y-%m-%d %H:%M")),
        format!("Système : {} {}", sysinfo::System::long_os_version().unwrap_or_else(|| std::env::consts::OS.into()), std::env::consts::ARCH),
        format!("Langue : {} (système : {})", snap["lang"], sys_locale::get_locale().unwrap_or_default()),
        format!("Port : {port}   identifiants Discord : {}   lancement avec Windows : {autostart}", if has_cfg { "oui" } else { "non" }),
        format!("Dossier des données : {}", hide_home(app.paths.dir.display().to_string())),
        format!("Mémoire utilisée : {mem} Mo   pages ouvertes (overlay + réglages) : {}", app.tx.receiver_count()),
        String::new(),
        format!("Discord : {}  {msg_en}", snap["status"].as_str().unwrap_or("")),
        format!("Salon vocal : {}  ({} personne(s))", if snap["channel"].is_null() { "aucun" } else { "oui" }, snap["members"].as_array().map_or(0, Vec::len)),
        format!("League : {}  {league_note}", snap["league"]["status"].as_str().unwrap_or("")),
        format!("DPM : {}   extensions autorisées : {}", snap["dpm"], if ext == 0 { "toutes".to_string() } else { ext.to_string() }),
        format!("Profils : {}   mise à jour : {}", snap["profiles"].as_object().map_or(0, |o| o.len()), snap["update"]),
        String::new(),
        "--- Fin du journal ---".into(),
    ];
    out.extend(crate::util::recent_log().into_iter().map(hide_home));
    out.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_names() {
        assert_eq!(safe_font_name("../../evil/My Font.TTF").as_deref(), Some("My Font.TTF"));
        assert_eq!(safe_font_name("C:\\x\\a.woff2").as_deref(), Some("a.woff2"));
        assert_eq!(safe_font_name("a.exe"), None);
        assert_eq!(safe_font_name(".ttf"), None);
        assert_eq!(safe_font_name("ça va.otf").as_deref(), Some("_a va.otf"));
    }

    #[test]
    fn uri_round_trip() {
        let n = "My Font (1).ttf";
        assert_eq!(decode_uri_component(&encode_uri_component(n)).unwrap(), n);
    }

    #[test]
    fn access_checks() {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, "localhost:47474".parse().unwrap());
        assert!(is_local(&h, 47474));
        h.insert(header::ORIGIN, "http://evil.com".parse().unwrap());
        assert!(!is_local(&h, 47474));
        h.insert(header::ORIGIN, "chrome-extension://abcdefghijklmnopabcdefghijklmnop".parse().unwrap());
        assert!(is_extension(&h, 47474, &[]));
        assert!(!is_extension(&h, 47474, &["pppppppppppppppppppppppppppppppp".into()]));
        h.insert(header::HOST, "evil.com:47474".parse().unwrap());
        assert!(!host_ok(&h, 47474));
    }

    #[test]
    fn css_lists_builtin_fonts() {
        let css = fonts_css(Path::new("/nonexistent"));
        assert!(css.contains("/webfonts/barlow-condensed-700.woff2"));
        assert!(css.contains("font-family:\"Bangers\";font-weight:100 900"));
    }
}
