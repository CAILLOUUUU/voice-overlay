//! Connexion à l'appli Discord (RPC local par « named pipe ») et suivi de ton salon vocal.

use crate::config::{token_clear, token_load, token_save};
use crate::state::{App, Channel, Member};
use crate::util::{m, M};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

const OP_HANDSHAKE: i32 = 0;
const OP_FRAME: i32 = 1;
const OP_CLOSE: i32 = 2;
const OP_PING: i32 = 3;
const OP_PONG: i32 = 4;
const MAX_FRAME: usize = 16_000_000;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// AUTHORIZE attend que tu cliques dans Discord : il a droit à plus de temps.
const AUTHORIZE_TIMEOUT: Duration = Duration::from_secs(300);

const CHANNEL_EVENTS: [&str; 5] = ["VOICE_STATE_CREATE", "VOICE_STATE_UPDATE", "VOICE_STATE_DELETE", "SPEAKING_START", "SPEAKING_STOP"];

#[derive(Debug)]
pub enum Code {
    NoDiscord,
    /// Erreur qui demande une action de l'utilisateur (mauvais identifiants, autorisation refusée…).
    Blocked,
    Other,
}

#[derive(Debug)]
pub struct Error {
    pub code: Code,
    pub text: String,
    pub msg: Option<M>,
}

impl Error {
    fn other(text: impl Into<String>) -> Self {
        Error { code: Code::Other, text: text.into(), msg: None }
    }
    fn blocked(text: impl Into<String>, msg: M) -> Self {
        Error { code: Code::Blocked, text: text.into(), msg: Some(msg) }
    }
}

trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

#[cfg(windows)]
async fn open_pipe(i: u32) -> Option<Box<dyn Stream>> {
    let name = format!(r"\\?\pipe\discord-ipc-{i}");
    tokio::net::windows::named_pipe::ClientOptions::new().open(name).ok().map(|p| Box::new(p) as Box<dyn Stream>)
}

#[cfg(unix)]
async fn open_pipe(i: u32) -> Option<Box<dyn Stream>> {
    let base = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .iter()
        .find_map(std::env::var_os)
        .map(|s| s.to_string_lossy().trim_end_matches('/').to_string())
        .unwrap_or_else(|| "/tmp".into());
    tokio::net::UnixStream::connect(format!("{base}/discord-ipc-{i}")).await.ok().map(|s| Box::new(s) as Box<dyn Stream>)
}

pub fn encode(op: i32, payload: &Value) -> Vec<u8> {
    let json = payload.to_string().into_bytes();
    let mut buf = Vec::with_capacity(8 + json.len());
    buf.extend_from_slice(&op.to_le_bytes());
    buf.extend_from_slice(&(json.len() as i32).to_le_bytes());
    buf.extend_from_slice(&json);
    buf
}

/// Ce que le lecteur transmet à la session.
#[derive(Debug)]
enum Incoming {
    Ready,
    Event(String, Value),
    /// Revérifier le salon choisi (filet de sécurité).
    Recheck,
    Closed(Option<String>),
}

type Pending = Arc<Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>>;

struct Rpc {
    out: mpsc::UnboundedSender<Vec<u8>>,
    pending: Pending,
    tasks: Vec<JoinHandle<()>>,
}

impl Drop for Rpc {
    fn drop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
    }
}

/// Lit les messages un par un. Un message illisible est ignoré ; une taille absurde coupe la connexion.
async fn reader(mut rd: impl AsyncRead + Unpin, out: mpsc::UnboundedSender<Vec<u8>>, pending: Pending, inc: mpsc::UnboundedSender<Incoming>) {
    let mut reason = None;
    loop {
        let mut head = [0u8; 8];
        if rd.read_exact(&mut head).await.is_err() {
            break;
        }
        let op = i32::from_le_bytes(head[0..4].try_into().unwrap());
        let len = i32::from_le_bytes(head[4..8].try_into().unwrap());
        if len < 0 || len as usize > MAX_FRAME {
            log::error!("[RPC] flux Discord corrompu : taille de message invalide");
            break;
        }
        let mut body = vec![0u8; len as usize];
        if rd.read_exact(&mut body).await.is_err() {
            break;
        }
        let Ok(msg) = serde_json::from_slice::<Value>(&body) else {
            log::error!("[RPC] message illisible ignoré");
            continue;
        };
        match op {
            OP_PING => {
                let _ = out.send(encode(OP_PONG, &msg));
            }
            OP_CLOSE => {
                reason = Some(msg.get("message").and_then(Value::as_str).map(String::from).unwrap_or_else(|| msg.to_string()));
                break;
            }
            _ => {
                let cmd = msg.get("cmd").and_then(Value::as_str).unwrap_or("");
                let evt = msg.get("evt").and_then(Value::as_str).unwrap_or("");
                if cmd == "DISPATCH" && evt == "READY" {
                    let _ = inc.send(Incoming::Ready);
                    continue;
                }
                if let Some(nonce) = msg.get("nonce").and_then(Value::as_str) {
                    let waiter = pending.lock().unwrap_or_else(|e| e.into_inner()).remove(nonce);
                    if let Some(tx) = waiter {
                        let data = msg.get("data").cloned().unwrap_or(Value::Null);
                        let r = if evt == "ERROR" {
                            Err(format!("{cmd} : {}", data.get("message").and_then(Value::as_str).unwrap_or("")))
                        } else {
                            Ok(data)
                        };
                        let _ = tx.send(r);
                        continue;
                    }
                }
                if cmd == "DISPATCH" {
                    let _ = inc.send(Incoming::Event(evt.to_string(), msg.get("data").cloned().unwrap_or(Value::Null)));
                }
            }
        }
    }
    // Les requêtes en attente échouent tout de suite (leurs canaux sont fermés).
    pending.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let _ = inc.send(Incoming::Closed(reason));
}

impl Rpc {
    async fn connect(client_id: &str) -> Result<(Rpc, mpsc::UnboundedSender<Incoming>, mpsc::UnboundedReceiver<Incoming>), Error> {
        let mut stream = None;
        for i in 0..10 {
            if let Some(s) = open_pipe(i).await {
                stream = Some(s);
                break;
            }
        }
        let Some(stream) = stream else {
            return Err(Error { code: Code::NoDiscord, text: "Discord introuvable".into(), msg: None });
        };
        let (rd, mut wr) = tokio::io::split(stream);
        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (inc_tx, inc_rx) = mpsc::unbounded_channel();
        let pending: Pending = Arc::default();
        let writer = tokio::spawn(async move {
            while let Some(buf) = out_rx.recv().await {
                if wr.write_all(&buf).await.is_err() {
                    break;
                }
            }
        });
        let reader = tokio::spawn(reader(rd, out_tx.clone(), pending.clone(), inc_tx.clone()));
        let _ = out_tx.send(encode(OP_HANDSHAKE, &json!({ "v": 1, "client_id": client_id })));
        Ok((Rpc { out: out_tx, pending, tasks: vec![writer, reader] }, inc_tx, inc_rx))
    }

    fn frame(cmd: &str, args: Value, evt: Option<&str>, nonce: &str) -> Vec<u8> {
        let mut p = json!({ "cmd": cmd, "args": args, "nonce": nonce });
        if let Some(e) = evt {
            p["evt"] = json!(e);
        }
        encode(OP_FRAME, &p)
    }

    async fn request_t(&self, cmd: &str, args: Value, evt: Option<&str>, timeout: Duration) -> Result<Value, String> {
        let nonce = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap_or_else(|e| e.into_inner()).insert(nonce.clone(), tx);
        let _ = self.out.send(Self::frame(cmd, args, evt, &nonce));
        let r = tokio::time::timeout(timeout, rx).await;
        self.pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&nonce);
        match r {
            Err(_) => Err(format!("{cmd} : pas de réponse de Discord")),
            Ok(Err(_)) => Err("Connexion Discord fermée".into()),
            Ok(Ok(r)) => r,
        }
    }

    async fn request(&self, cmd: &str, args: Value, evt: Option<&str>) -> Result<Value, String> {
        self.request_t(cmd, args, evt, REQUEST_TIMEOUT).await
    }

    /// Envoi sans attendre la réponse.
    fn fire(&self, cmd: &str, args: Value, evt: Option<&str>) {
        let _ = self.out.send(Self::frame(cmd, args, evt, &uuid::Uuid::new_v4().to_string()));
    }
}

fn closed_error(reason: Option<String>) -> Error {
    match reason {
        None => Error::other("Connexion Discord fermée"),
        Some(r) => {
            let lower = r.to_lowercase();
            let msg = if lower.contains("client id") || lower.contains("client_id") || lower.contains("clientid") {
                m("Discord ne reconnaît pas ce Client ID. Vérifie que tu l'as bien copié.", "Discord does not recognize this Client ID. Check that you copied it correctly.")
            } else {
                m(format!("Discord a refusé la connexion : {r}"), format!("Discord refused the connection: {r}"))
            };
            Error::blocked(r, msg)
        }
    }
}

// ---------- Authentification (une seule fois, puis jeton sauvegardé) ----------

async fn exchange_token(app: &App, params: &[(&str, &str)]) -> Result<Value, Error> {
    let (id, secret) = {
        let c = app.config.lock().unwrap();
        (c.client_id.clone(), c.client_secret.clone())
    };
    let mut form: Vec<(&str, &str)> = vec![("client_id", &id), ("client_secret", &secret)];
    form.extend_from_slice(params);
    let r = reqwest::Client::new()
        .post("https://discord.com/api/oauth2/token")
        .form(&form)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| Error::other(format!("token: {e}")))?;
    let status = r.status();
    let j: Value = r.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        let err = j.get("error").and_then(Value::as_str).map(String::from).unwrap_or_else(|| status.as_u16().to_string());
        let msg = if err == "invalid_client" {
            m("Le Client Secret n'est pas le bon. Refais « Reset Secret » sur le site Discord et colle le nouveau.", "The Client Secret is wrong. Click \"Reset Secret\" on the Discord site again and paste the new one.")
        } else {
            m(
                format!("Discord a refusé les codes ({err}). Vérifie le Client ID, le Client Secret et l'adresse http://localhost dans « Redirects »."),
                format!("Discord rejected the codes ({err}). Check the Client ID, the Client Secret and the http://localhost address under \"Redirects\"."),
            )
        };
        return Err(Error::blocked(format!("token: {err}"), msg));
    }
    token_save(&app.paths, &j);
    Ok(j)
}

fn access_token(tok: &Value) -> Value {
    json!({ "access_token": tok.get("access_token").cloned().unwrap_or(Value::Null) })
}

async fn authenticate(app: &App, rpc: &Rpc) -> Result<Value, Error> {
    if let Some(tok) = token_load(&app.paths) {
        if let Ok(v) = rpc.request("AUTHENTICATE", access_token(&tok), None).await {
            return Ok(v);
        }
        if let Some(refresh) = tok.get("refresh_token").and_then(Value::as_str) {
            if let Ok(tok) = exchange_token(app, &[("grant_type", "refresh_token"), ("refresh_token", refresh)]).await {
                if let Ok(v) = rpc.request("AUTHENTICATE", access_token(&tok), None).await {
                    return Ok(v);
                }
            }
        }
    }
    app.update(|i| i.set_status("auth", Some(m("Une fenêtre s'est ouverte dans Discord : clique sur \"Autoriser\".", "A window opened in Discord: click \"Authorize\"."))));
    let client_id = app.config.lock().unwrap().client_id.clone();
    let code = match rpc.request_t("AUTHORIZE", json!({ "client_id": client_id, "scopes": ["rpc", "identify"] }), None, AUTHORIZE_TIMEOUT).await {
        Ok(v) => v.get("code").and_then(Value::as_str).unwrap_or("").to_string(),
        Err(e) => {
            return Err(Error::blocked(
                e.clone(),
                m(
                    format!("L'autorisation Discord a été refusée ou fermée. Clique sur « Réessayer », puis sur « Autoriser » dans Discord. ({e})"),
                    format!("The Discord authorization was declined or closed. Click \"Try again\", then \"Authorize\" in Discord. ({e})"),
                ),
            ))
        }
    };
    let redirect = app.config.lock().unwrap().redirect_uri.clone();
    let tok = exchange_token(app, &[("grant_type", "authorization_code"), ("code", &code), ("redirect_uri", &redirect)]).await?;
    rpc.request("AUTHENTICATE", access_token(&tok), None).await.map_err(Error::other)
}

// ---------- Conversion d'un « voice state » Discord ----------

fn sv<'a>(v: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut cur = v;
    for k in path {
        cur = cur.get(k)?;
    }
    cur.as_str().filter(|s| !s.is_empty())
}

pub fn to_member(vs: &Value, guild_id: Option<&str>) -> Option<Member> {
    let u = vs.get("user")?;
    let id = sv(u, &["id"])?.to_string();
    let v = vs.get("voice_state").cloned().unwrap_or(json!({}));
    let flag = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
    // Avatar de serveur si Discord le fournit, sinon avatar du compte, sinon avatar par défaut.
    let guild_avatar = sv(vs, &["member", "avatar"]).or_else(|| sv(vs, &["guild_avatar"])).or_else(|| sv(u, &["guild_avatar"]));
    let avatar = match (guild_avatar, guild_id) {
        (Some(a), Some(g)) => format!("https://cdn.discordapp.com/guilds/{g}/users/{id}/avatars/{a}.png?size=128"),
        _ => match sv(u, &["avatar"]) {
            Some(a) => format!("https://cdn.discordapp.com/avatars/{id}/{a}.png?size=128"),
            None => format!("https://cdn.discordapp.com/embed/avatars/{}.png", (id.parse::<u64>().unwrap_or(0) >> 22) % 6),
        },
    };
    let deco = sv(u, &["avatar_decoration_data", "asset"])
        .filter(|a| a.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        .map(|a| format!("https://cdn.discordapp.com/avatar-decoration-presets/{a}.png?size=160&passthrough=true"));
    let name = sv(vs, &["nick"]).or_else(|| sv(u, &["global_name"])).or_else(|| sv(u, &["username"])).unwrap_or("?").to_string();
    Some(Member { id, name, avatar, deco, mute: flag("mute") || flag("self_mute"), deaf: flag("deaf") || flag("self_deaf") })
}

// ---------- Suivi du salon vocal ----------

struct Session<'a> {
    app: &'a Arc<App>,
    rpc: Rpc,
    current: Option<String>,
    inc_tx: mpsc::UnboundedSender<Incoming>,
}

impl Session<'_> {
    async fn follow(&mut self, channel_id: Option<String>) -> Result<(), String> {
        if channel_id == self.current {
            return Ok(());
        }
        if let Some(old) = &self.current {
            for evt in CHANNEL_EVENTS {
                self.rpc.fire("UNSUBSCRIBE", json!({ "channel_id": old }), Some(evt));
            }
        }
        self.current = channel_id.clone();
        self.app.update(|i| i.reset_channel());
        let Some(id) = channel_id else {
            log::info!("Tu n'es dans aucun vocal.");
            self.app.update(|i| {
                i.prune_people();
                i.broadcast();
            });
            return Ok(());
        };
        let ch = self.rpc.request("GET_CHANNEL", json!({ "channel_id": id }), None).await?;
        let guild = sv(&ch, &["guild_id"]).map(String::from);
        let name = sv(&ch, &["name"]).unwrap_or("").to_string();
        let n = self.app.update(|i| {
            i.channel = Some(Channel { id: sv(&ch, &["id"]).unwrap_or(&id).to_string(), name: name.clone(), guild_id: guild.clone() });
            for vs in ch.get("voice_states").and_then(Value::as_array).into_iter().flatten() {
                if let Some(m) = to_member(vs, guild.as_deref()) {
                    i.upsert_member(m);
                }
            }
            i.prune_people(); // on garde les réglages de ceux qui t'ont suivi dans le nouveau salon
            i.members.len()
        });
        for evt in CHANNEL_EVENTS {
            self.rpc.request("SUBSCRIBE", json!({ "channel_id": id }), Some(evt)).await?;
        }
        log::info!("Vocal suivi : {name} ({n} personne(s))");
        self.app.update(|i| i.broadcast());
        Ok(())
    }

    /// Filet de sécurité : si Discord ne prévient pas qu'on a quitté le vocal, on revérifie.
    fn recheck_soon(&self) {
        let tx = self.inc_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(1200)).await;
            let _ = tx.send(Incoming::Recheck);
        });
    }

    async fn selected(&self) -> Option<String> {
        let sel = self.rpc.request("GET_SELECTED_VOICE_CHANNEL", json!({}), None).await.ok()?;
        sv(&sel, &["id"]).map(String::from)
    }

    async fn on_event(&mut self, evt: &str, data: Value) {
        let current = self.current.clone();
        match evt {
            "VOICE_CHANNEL_SELECT" => {
                let id = sv(&data, &["channel_id"]).map(String::from);
                if let Err(e) = self.follow(id).await {
                    log::error!("[Suivi] {e}");
                }
            }
            "VOICE_STATE_CREATE" | "VOICE_STATE_UPDATE" => {
                self.app.update(|i| {
                    let guild = i.channel.as_ref().and_then(|c| c.guild_id.clone());
                    let Some(m) = to_member(&data, guild.as_deref()) else { return };
                    let id = m.id.clone();
                    let is_new = !i.has_member(&id);
                    i.upsert_member(m);
                    if is_new {
                        i.dpm_join(&id);
                        i.dpm_correlate();
                    }
                    i.broadcast();
                });
            }
            "VOICE_STATE_DELETE" => {
                let Some(id) = sv(&data, &["user", "id"]).map(String::from) else { return };
                let is_self = self.app.update(|i| {
                    i.remove_member(&id);
                    i.prune_people();
                    i.broadcast();
                    i.self_id.as_deref() == Some(id.as_str())
                });
                if is_self {
                    self.recheck_soon();
                }
            }
            "SPEAKING_START" => {
                if let Some(ch) = sv(&data, &["channel_id"]) {
                    if Some(ch) != current.as_deref() {
                        return;
                    }
                }
                if let Some(uid) = sv(&data, &["user_id"]).map(String::from) {
                    self.app.update(|i| {
                        if i.speaking.insert(uid) {
                            i.broadcast_speaking();
                        }
                    });
                }
            }
            "SPEAKING_STOP" => {
                if let Some(uid) = sv(&data, &["user_id"]) {
                    self.app.update(|i| {
                        if i.speaking.remove(uid) {
                            i.broadcast_speaking();
                        }
                    });
                }
            }
            _ => {}
        }
    }
}

/// Une connexion complète : se termine quand Discord se ferme ou qu'on demande de recommencer.
async fn session(app: &Arc<App>) -> Result<(), Error> {
    let client_id = app.config.lock().unwrap().client_id.clone();
    let (rpc, inc_tx, mut inc) = Rpc::connect(&client_id).await?;
    // Attente du READY (ou d'un refus) après la poignée de main.
    match tokio::time::timeout(REQUEST_TIMEOUT, inc.recv()).await {
        Ok(Some(Incoming::Ready)) => {}
        Ok(Some(Incoming::Closed(r))) => return Err(closed_error(r)),
        Ok(_) => return Err(Error::other("Réponse inattendue de Discord")),
        Err(_) => return Err(Error::other("Discord ne répond pas")),
    }
    // L'authentification peut attendre ton clic dans Discord : un refus de Discord l'interrompt.
    let auth = tokio::select! {
        r = authenticate(app, &rpc) => r?,
        r = wait_closed(&mut inc) => return Err(closed_error(r)),
        _ = app.restart.notified() => return Ok(()),
    };
    let user = auth.get("user").cloned().unwrap_or(Value::Null);
    let self_id = sv(&user, &["id"]).map(String::from);
    let name = sv(&user, &["global_name"]).or_else(|| sv(&user, &["username"])).unwrap_or("?").to_string();
    app.update(|i| {
        i.self_id = self_id;
        i.user = Some(name.clone());
        i.set_status("connected", Some(m(format!("Connecté à Discord en tant que {name}."), format!("Connected to Discord as {name}."))));
    });
    let mut s = Session { app, rpc, current: None, inc_tx };
    s.rpc.request("SUBSCRIBE", json!({}), Some("VOICE_CHANNEL_SELECT")).await.map_err(Error::other)?;
    let sel = s.selected().await;
    if let Err(e) = s.follow(sel).await {
        log::error!("[Suivi] {e}");
    }
    loop {
        tokio::select! {
            msg = inc.recv() => match msg {
                Some(Incoming::Event(evt, data)) => s.on_event(&evt, data).await,
                Some(Incoming::Recheck) => {
                    let sel = s.selected().await;
                    if let Err(e) = s.follow(sel).await { log::error!("[Suivi] {e}"); }
                }
                Some(Incoming::Ready) => {}
                Some(Incoming::Closed(r)) => return match r { Some(_) => Err(closed_error(r)), None => Ok(()) },
                None => return Ok(()),
            },
            _ = app.restart.notified() => return Ok(()),
        }
    }
}

async fn wait_closed(inc: &mut mpsc::UnboundedReceiver<Incoming>) -> Option<String> {
    loop {
        match inc.recv().await {
            Some(Incoming::Closed(r)) => return r,
            None => return None,
            _ => {}
        }
    }
}

pub async fn run(app: Arc<App>) {
    loop {
        app.restart_requested.store(false, Ordering::SeqCst);
        if !app.config.lock().unwrap().has_credentials() {
            app.update(|i| i.set_status("setup", Some(m("Identifiants Discord manquants : ouvre la page de réglages pour les saisir.", "Discord credentials missing: open the settings page to enter them."))));
            app.pause(3_600_000).await;
            continue;
        }
        let mut blocked = false;
        let r = session(&app).await;
        let restarted = app.restart_requested.load(Ordering::SeqCst);
        match r {
            Ok(()) if !restarted => app.update(|i| i.set_status("no-discord", Some(m("Connexion à Discord perdue, nouvelle tentative…", "Lost connection to Discord, retrying…")))),
            Ok(()) => {}
            Err(_) if restarted => {}
            Err(e) => app.update(|i| match e.code {
                Code::NoDiscord => i.set_status("no-discord", Some(m("Discord introuvable : l'appli Discord (desktop) est bien ouverte ?", "Discord not found: is the Discord desktop app open?"))),
                Code::Blocked | Code::Other => {
                    blocked = matches!(e.code, Code::Blocked);
                    let msg = e.msg.clone().unwrap_or_else(|| m(e.text.clone(), e.text.clone()));
                    i.set_status("error", Some(msg));
                }
            }),
        }
        app.update(|i| {
            i.reset_channel();
            i.prune_people();
            i.broadcast();
        });
        // Erreur qui demande une action de l'utilisateur : on attend qu'il corrige ou clique sur « Réessayer ».
        app.pause(if blocked { 3_600_000 } else { 5_000 }).await;
    }
}

/// Nouveaux identifiants : on oublie l'ancien jeton et on se reconnecte.
pub fn credentials_changed(app: &App) {
    token_clear(&app.paths);
    app.request_restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_from_voice_state() {
        let vs = json!({
            "nick": "Surnom",
            "user": { "id": "80351110224678912", "username": "u", "global_name": "G", "avatar": null,
                      "avatar_decoration_data": { "asset": "a_123" } },
            "voice_state": { "self_mute": true, "deaf": false }
        });
        let m = to_member(&vs, Some("1")).unwrap();
        assert_eq!(m.name, "Surnom");
        assert!(m.mute && !m.deaf);
        assert_eq!(m.avatar, format!("https://cdn.discordapp.com/embed/avatars/{}.png", (80351110224678912u64 >> 22) % 6));
        assert!(m.deco.unwrap().contains("a_123"));
    }

    #[test]
    fn rejects_unsafe_decoration_asset() {
        let vs = json!({ "user": { "id": "1", "username": "u", "avatar_decoration_data": { "asset": "../x" } } });
        assert!(to_member(&vs, None).unwrap().deco.is_none());
    }

    #[test]
    fn frame_encoding() {
        let b = encode(OP_FRAME, &json!({ "a": 1 }));
        assert_eq!(&b[0..4], &1i32.to_le_bytes());
        assert_eq!(&b[4..8], &7i32.to_le_bytes());
        assert_eq!(&b[8..], br#"{"a":1}"#);
    }

    /// Faux Discord : un message illisible ne casse pas le flux, la réponse suivante arrive bien.
    #[tokio::test]
    async fn reader_survives_garbage_and_routes_responses() {
        let (client, mut server) = tokio::io::duplex(4096);
        let (rd, _wr) = tokio::io::split(client);
        let (out_tx, _out_rx) = mpsc::unbounded_channel();
        let (inc_tx, mut inc_rx) = mpsc::unbounded_channel();
        let pending: Pending = Arc::default();
        let (tx, rx) = oneshot::channel();
        pending.lock().unwrap().insert("n1".into(), tx);
        tokio::spawn(reader(rd, out_tx, pending.clone(), inc_tx));
        let mut garbage = 1i32.to_le_bytes().to_vec();
        garbage.extend_from_slice(&3i32.to_le_bytes());
        garbage.extend_from_slice(b"{{{");
        server.write_all(&garbage).await.unwrap();
        server.write_all(&encode(OP_FRAME, &json!({ "cmd": "DISPATCH", "evt": "READY" }))).await.unwrap();
        server.write_all(&encode(OP_FRAME, &json!({ "cmd": "GET_CHANNEL", "nonce": "n1", "data": { "id": "42" } }))).await.unwrap();
        assert!(matches!(inc_rx.recv().await, Some(Incoming::Ready)));
        assert_eq!(rx.await.unwrap().unwrap()["id"], "42");
        server.write_all(&encode(OP_CLOSE, &json!({ "message": "Invalid Client ID" }))).await.unwrap();
        match inc_rx.recv().await {
            Some(Incoming::Closed(Some(r))) => assert!(matches!(closed_error(Some(r)).code, Code::Blocked)),
            other => panic!("{other:?}"),
        }
    }
}
