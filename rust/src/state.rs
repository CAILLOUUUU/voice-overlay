//! État partagé de l'appli et envoi des mises à jour aux pages (SSE).

use crate::config::{Config, Paths};
use crate::settings::{self, Custom, Profile, RiotLink, ROLES};
use crate::util::{now_ms, write_json_atomic, M};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::{broadcast, Notify};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_NAME: &str = "VOICE OVERLAY - CAILLOU FEAT CLAUDE";
pub const REPO: &str = "CAILLOUUUU/voice-overlay";

#[derive(Clone, Debug, Serialize)]
pub struct Member {
    pub id: String,
    pub name: String,
    pub avatar: String,
    pub deco: Option<String>,
    pub mute: bool,
    pub deaf: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Channel {
    pub id: String,
    pub name: String,
    #[serde(rename = "guildId")]
    pub guild_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Update {
    pub version: String,
    pub url: String,
    /// La page peut proposer « Installer » (sinon seulement le lien de téléchargement).
    #[serde(rename = "canInstall")]
    pub can_install: bool,
}

#[derive(Default)]
pub struct League {
    /// off (client fermé) | on (client ouvert) | game (en partie)
    pub status: &'static str,
    /// id Discord -> rôle trouvé automatiquement
    pub roles: HashMap<String, String>,
    /// Ton propre Riot ID, lu dans le client League.
    pub me: Option<RiotLink>,
    /// Dernier diagnostic de détection (pour « Copier le diagnostic »).
    pub note: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DpmPlayer {
    pub key: String,
    pub name: String,
    pub tag: String,
    pub side: String,
    pub lane: String,
    pub in_voice: bool,
}

#[derive(Clone, Debug)]
pub struct Lobby {
    pub match_id: String,
    pub state: String,
    pub players: Vec<DpmPlayer>,
    pub you_side: String,
    pub at: i64,
}

#[derive(Default)]
pub struct Dpm {
    /// id Discord -> Riot ID
    pub links: HashMap<String, RiotLink>,
    pub lobby: Option<Lobby>,
    /// pastilles passées au vert pendant ce lobby : (clé Riot, instant)
    pub greens: Vec<(String, i64)>,
    /// arrivées dans le vocal pendant ce lobby : (id Discord, instant)
    pub joins: Vec<(String, i64)>,
    pub last_extension: i64,
    /// liens oubliés à la main : « idDiscord|riotKey », pas recréés avant le prochain lobby
    pub blocked: HashSet<String>,
}

pub struct Inner {
    pub status: &'static str,
    pub status_msg: Option<M>,
    pub user: Option<String>,
    pub channel: Option<Channel>,
    pub self_id: Option<String>,
    /// dans l'ordre d'arrivée
    pub members: Vec<Member>,
    pub speaking: HashSet<String>,
    pub custom: HashMap<String, Custom>,
    pub order: Vec<String>,
    pub last_seen: HashMap<String, i64>,
    pub lang: String,
    pub profiles: Vec<Profile>,
    pub forget_after: i64,
    pub update: Option<Update>,
    pub update_asset: Option<crate::updater::Asset>,
    /// downloading | restarting | error (+ message)
    pub update_status: Option<(&'static str, Option<M>)>,
    pub assets_version: i64,
    pub league: League,
    pub dpm: Dpm,
    pub ingame: settings::Ingame,
    /// L'overlay en jeu est affiché pour être déplacé (raccourci ou bouton des réglages).
    pub placing: bool,
    /// Le raccourci a pu être réservé auprès de Windows (sinon un autre programme l'utilise).
    pub hotkey_ok: bool,
    // Drapeaux remis à zéro par App::update
    dirty: bool,
    speaking_dirty: bool,
    people_dirty: bool,
}

impl Inner {
    pub fn new(loaded: settings::Loaded, demo: bool) -> Self {
        Inner {
            status: if demo { "demo" } else { "starting" },
            status_msg: None,
            user: None,
            channel: None,
            self_id: None,
            members: vec![],
            speaking: HashSet::new(),
            custom: loaded.custom,
            order: loaded.order,
            last_seen: loaded.last_seen,
            lang: loaded.lang,
            profiles: loaded.profiles,
            forget_after: loaded.forget_after,
            update: None,
            update_asset: None,
            update_status: None,
            assets_version: now_ms(),
            league: League { status: "off", ..Default::default() },
            dpm: Dpm { links: loaded.links, ..Default::default() },
            ingame: loaded.ingame,
            placing: false,
            hotkey_ok: true,
            dirty: false,
            speaking_dirty: false,
            people_dirty: false,
        }
    }

    /// Les pages recevront l'état complet.
    pub fn broadcast(&mut self) {
        self.dirty = true;
    }
    /// Seulement « qui parle » a changé.
    pub fn broadcast_speaking(&mut self) {
        self.speaking_dirty = true;
    }
    /// Renommages, ordre ou liens à réécrire sur le disque (regroupé, au plus une fois par seconde).
    pub fn save_people_soon(&mut self) {
        self.people_dirty = true;
    }

    pub fn member(&self, id: &str) -> Option<&Member> {
        self.members.iter().find(|m| m.id == id)
    }
    pub fn has_member(&self, id: &str) -> bool {
        self.member(id).is_some()
    }
    pub fn upsert_member(&mut self, m: Member) {
        match self.members.iter_mut().find(|x| x.id == m.id) {
            Some(x) => *x = m,
            None => self.members.push(m),
        }
    }
    pub fn remove_member(&mut self, id: &str) {
        self.members.retain(|m| m.id != id);
        self.speaking.remove(id);
    }

    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }
    pub fn new_profile_id(&self, name: &str) -> String {
        let base = settings::profile_id_base(name);
        let mut id = base.clone();
        let mut i = 2;
        while self.profile(&id).is_some() || id == settings::INGAME {
            id = format!("{base}-{i}");
            i += 1;
        }
        id
    }

    pub fn set_status(&mut self, status: &'static str, msg: Option<M>) {
        if self.status == status && self.status_msg == msg {
            return;
        }
        if let Some(m) = &msg {
            log::info!("{}", m.fr);
        }
        self.status = status;
        self.status_msg = msg;
        self.broadcast();
    }

    pub fn reset_channel(&mut self) {
        self.channel = None;
        self.members.clear();
        self.speaking.clear();
    }

    /// Rôle : celui choisi à la main, sinon DPM, sinon celui trouvé dans le client League.
    pub fn auto_role(&self, id: &str) -> String {
        let d = self.dpm_role(id);
        if !d.is_empty() {
            return d;
        }
        self.league.roles.get(id).cloned().unwrap_or_default()
    }
    pub fn role_of(&self, id: &str) -> String {
        match self.custom.get(id) {
            Some(c) if !c.role.is_empty() => c.role.clone(),
            _ => self.auto_role(id),
        }
    }

    /// D'abord les rôles (Top → Support), puis les autres selon l'ordre personnalisé.
    pub fn ordered_members(&self) -> Vec<&Member> {
        let pos: HashMap<&str, usize> = self.order.iter().enumerate().map(|(i, id)| (id.as_str(), i)).collect();
        let mut v: Vec<(i64, &Member)> = self
            .members
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let r = ROLES.iter().position(|x| *x == self.role_of(&m.id));
                let k = match (r, pos.get(m.id.as_str())) {
                    (Some(r), _) => r as i64 - 100,
                    (None, Some(p)) => *p as i64,
                    (None, None) => 1_000_000 + i as i64,
                };
                (k, m)
            })
            .collect();
        v.sort_by_key(|x| x.0); // tri stable, comme en JS
        v.into_iter().map(|x| x.1).collect()
    }

    pub fn snapshot(&self) -> Value {
        let members: Vec<Value> = self
            .ordered_members()
            .into_iter()
            .map(|m| {
                let c = self.custom.get(&m.id).cloned().unwrap_or_default();
                json!({
                    "id": m.id, "name": if c.name.is_empty() { &m.name } else { &c.name },
                    "avatar": m.avatar, "deco": m.deco, "mute": m.mute, "deaf": m.deaf,
                    "realName": m.name, "hidden": c.hidden, "speaking": self.speaking.contains(&m.id),
                    "role": self.role_of(&m.id), "manualRole": c.role, "autoRole": self.auto_role(&m.id),
                    "riot": self.dpm.links.get(&m.id), "side": self.dpm_side(&m.id),
                })
            })
            .collect();
        json!({
            "status": self.status,
            "statusMsg": self.status_msg,
            "user": self.user,
            "version": VERSION,
            "update": self.update,
            "updateStatus": self.update_status.as_ref().map(|(s, e)| json!({ "state": s, "error": e })),
            "lang": self.lang,
            "profiles": self.profiles.iter().map(|p| (p.id.clone(), Value::Object(p.settings.clone())))
                .chain(std::iter::once((settings::INGAME.to_string(), Value::Object(self.ingame.settings.clone()))))
                .collect::<serde_json::Map<_, _>>(),
            "ingame": { "enabled": self.ingame.enabled, "placing": self.placing, "hotkey": self.ingame.hotkey, "hotkeyOk": self.hotkey_ok },
            "defaultProfile": self.profiles[0].id,
            "assetsVersion": self.assets_version,
            "channel": self.channel,
            "selfId": self.self_id,
            "members": members,
            "league": { "status": self.league.status },
            "dpm": self.dpm_status(),
        })
    }

    pub fn speaking_json(&self) -> String {
        let ids: Vec<&String> = self.members.iter().map(|m| &m.id).filter(|id| self.speaking.contains(*id)).collect();
        json!({ "speaking": ids }).to_string()
    }

    /// Oublie les réglages des personnes parties depuis plus de « forgetAfter » minutes.
    /// Une coupure de Discord (chez toi ou chez quelqu'un du vocal) ne fait donc pas tout perdre.
    pub fn prune_people(&mut self) {
        let now = now_ms();
        let tracked: HashSet<String> = self.custom.keys().cloned().chain(self.order.iter().cloned()).collect();
        for id in &tracked {
            if self.has_member(id) || !self.last_seen.contains_key(id) {
                self.last_seen.insert(id.clone(), now);
            }
        }
        self.last_seen.retain(|id, _| tracked.contains(id));
        self.save_people_soon();
        // Pendant une coupure avec Discord, on ne sait pas qui est là : on n'efface rien.
        let live = self.status == "connected" || self.status == "demo";
        if !live || self.forget_after < 0 {
            return;
        }
        let limit = self.forget_after * 60_000;
        for id in &tracked {
            if self.has_member(id) || now - self.last_seen.get(id).copied().unwrap_or(now) < limit {
                continue;
            }
            self.custom.remove(id);
            self.last_seen.remove(id);
        }
        let keep: Vec<String> = self
            .order
            .iter()
            .filter(|id| self.custom.contains_key(*id) || self.has_member(id) || self.last_seen.contains_key(*id))
            .cloned()
            .collect();
        self.order = keep;
    }

    /// Contenu de settings.json.
    pub fn store_json(&self) -> Value {
        json!({
            "lang": self.lang,
            "profiles": self.profiles,
            "people": {
                "forgetAfter": self.forget_after,
                "custom": self.custom,
                "order": self.order,
                "lastSeen": self.last_seen,
                "links": self.dpm.links,
            },
            "ingame": self.ingame,
        })
    }
}

/// Message envoyé aux pages ouvertes.
#[derive(Clone, Debug)]
pub enum Push {
    Full(Arc<String>),
    Speaking(Arc<String>),
}

pub struct App {
    inner: Mutex<Inner>,
    pub config: Mutex<Config>,
    pub paths: Paths,
    pub tx: broadcast::Sender<Push>,
    full_wanted: Notify,
    full_pending: AtomicBool,
    /// « Réessayer » ou nouveaux identifiants : la connexion Discord repart tout de suite.
    pub restart: Notify,
    pub restart_requested: AtomicBool,
}

impl App {
    pub fn new(inner: Inner, config: Config, paths: Paths) -> Arc<Self> {
        let (tx, _) = broadcast::channel(64);
        Arc::new(App {
            inner: Mutex::new(inner),
            config: Mutex::new(config),
            paths,
            tx,
            full_wanted: Notify::new(),
            full_pending: AtomicBool::new(false),
            restart: Notify::new(),
            restart_requested: AtomicBool::new(false),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // Une panique ailleurs ne doit pas bloquer toute l'appli.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Lecture seule.
    pub fn read<R>(&self, f: impl FnOnce(&Inner) -> R) -> R {
        f(&self.lock())
    }

    /// Modification, puis envoi aux pages si besoin.
    pub fn update<R>(&self, f: impl FnOnce(&mut Inner) -> R) -> R {
        let (r, full, speaking) = {
            let mut g = self.lock();
            let r = f(&mut g);
            let full = std::mem::take(&mut g.dirty);
            let sp = std::mem::take(&mut g.speaking_dirty);
            let speaking = (sp && !full).then(|| g.speaking_json());
            (r, full, speaking)
        };
        if full {
            self.full_pending.store(true, Ordering::SeqCst);
            self.full_wanted.notify_one();
        } else if let Some(s) = speaking {
            if !self.full_pending.load(Ordering::SeqCst) {
                let _ = self.tx.send(Push::Speaking(Arc::new(s)));
            }
        }
        r
    }

    pub fn snapshot_string(&self) -> String {
        self.read(|i| i.snapshot().to_string())
    }

    /// Plusieurs changements rapprochés ne donnent qu'un seul envoi de l'état complet.
    pub async fn broadcaster(self: Arc<Self>) {
        loop {
            self.full_wanted.notified().await;
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            self.full_pending.store(false, Ordering::SeqCst);
            let s = self.snapshot_string();
            let _ = self.tx.send(Push::Full(Arc::new(s)));
        }
    }

    pub fn save_store(&self) {
        let v = self.read(|i| i.store_json());
        if let Err(e) = write_json_atomic(&self.paths.settings, &v) {
            log::error!("Impossible d'enregistrer settings.json : {e}");
        }
    }

    /// Écrit settings.json quand les données par personne ont changé (au plus une fois par seconde).
    pub async fn people_saver(self: Arc<Self>) {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let v = {
                let mut g = self.lock();
                if !std::mem::take(&mut g.people_dirty) {
                    continue;
                }
                g.store_json()
            };
            if let Err(e) = write_json_atomic(&self.paths.settings, &v) {
                log::error!("Impossible d'enregistrer settings.json : {e}");
            }
        }
    }

    /// Toutes les 20 s, oublie les personnes parties depuis longtemps.
    pub async fn pruner(self: Arc<Self>) {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(20)).await;
            self.update(|i| {
                let n = i.custom.len() + i.order.len();
                i.prune_people();
                if i.custom.len() + i.order.len() != n {
                    i.broadcast();
                }
            });
        }
    }

    pub fn request_restart(&self) {
        self.restart_requested.store(true, Ordering::SeqCst);
        // Pas de « permis » gardé en réserve : sinon la connexion suivante s'arrêterait aussitôt.
        self.restart.notify_waiters();
    }

    /// Attend `ms`, mais repart tout de suite si on demande une nouvelle connexion.
    pub async fn pause(&self, ms: u64) {
        if self.restart_requested.load(Ordering::SeqCst) {
            return;
        }
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(ms)) => {}
            _ = self.restart.notified() => {}
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn member(id: &str, name: &str) -> Member {
        Member { id: id.into(), name: name.into(), avatar: String::new(), deco: None, mute: false, deaf: false }
    }

    pub fn inner() -> Inner {
        let mut i = Inner::new(settings::load(None), true);
        i.status = "connected";
        i
    }

    #[test]
    fn order_roles_first_then_custom_order() {
        let mut i = inner();
        for (id, n) in [("10001", "a"), ("10002", "b"), ("10003", "c"), ("10004", "d")] {
            i.upsert_member(member(id, n));
        }
        i.order = vec!["10003".into(), "10001".into()];
        i.custom.insert("10004".into(), Custom { role: "mid".into(), ..Default::default() });
        let ids: Vec<&str> = i.ordered_members().iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["10004", "10003", "10001", "10002"]);
    }

    #[test]
    fn prune_forgets_after_delay_only_when_live() {
        let mut i = inner();
        i.forget_after = 2;
        i.custom.insert("10001".into(), Custom { name: "Bob".into(), ..Default::default() });
        i.last_seen.insert("10001".into(), now_ms() - 3 * 60_000);
        i.status = "no-discord";
        i.prune_people();
        assert!(i.custom.contains_key("10001"), "pas d'oubli pendant une coupure");
        i.status = "connected";
        i.prune_people();
        assert!(!i.custom.contains_key("10001"));
    }

    #[test]
    fn snapshot_shape() {
        let mut i = inner();
        i.upsert_member(member("10001", "Real"));
        i.custom.insert("10001".into(), Custom { name: "Shown".into(), ..Default::default() });
        let s = i.snapshot();
        assert_eq!(s["members"][0]["name"], "Shown");
        assert_eq!(s["members"][0]["realName"], "Real");
        assert_eq!(s["defaultProfile"], "principal");
        assert!(s["profiles"]["principal"]["size"].is_number());
        assert_eq!(s["dpm"]["links"], 0);
    }
}
