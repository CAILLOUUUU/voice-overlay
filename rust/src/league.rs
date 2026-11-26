//! Rôles League of Legends (lecture du client League ouvert sur ce PC).
//!
//! L'overlay lit, en local et sans rien envoyer à Riot, le salon de la partie et la sélection
//! des champions, puis relie chaque Riot ID à la personne du vocal Discord dont le pseudo ressemble.
//! API locale non officielle du client : si Riot la change, le choix manuel du rôle reste possible.

use crate::settings::RiotLink;
use crate::similarity::similarity;
use crate::state::{App, Inner};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    pub puuid: String,
    pub name: String,
    pub tag: String,
    pub role: String,
}

pub fn to_role(v: &str) -> &'static str {
    let k: String = v.to_lowercase().chars().filter(char::is_ascii_lowercase).collect();
    match k.as_str() {
        "top" | "toplane" => "top",
        "jungle" | "jungler" | "jgl" | "jg" => "jungle",
        "mid" | "middle" | "midlane" => "mid",
        "adc" | "bottom" | "bot" | "carry" | "marksman" => "adc",
        "support" | "utility" | "supp" | "sup" => "support",
        _ => "",
    }
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}
fn first<'a>(v: &'a Value, keys: &[&str]) -> &'a str {
    keys.iter().map(|k| s(v, k)).find(|x| !x.is_empty()).unwrap_or("")
}

/// Parcourt une réponse du client et récupère les joueurs qui ont un rôle, quel que soit le format exact.
pub fn collect_players(node: &Value, out: &mut Vec<Player>, depth: usize) {
    if depth > 8 {
        return;
    }
    match node {
        Value::Array(a) => a.iter().for_each(|x| collect_players(x, out, depth + 1)),
        Value::Object(o) => {
            let role = to_role(first(node, &["assignedPosition", "position", "selectedPosition", "firstPositionPreference", "teamPosition"]));
            let riot = first(node, &["riotId", "riotIdGameName", "gameName", "summonerName"]).trim();
            let puuid = s(node, "puuid");
            if (!puuid.is_empty() || !riot.is_empty()) && !role.is_empty() {
                let (name, tag) = match riot.split_once('#') {
                    Some((n, t)) => (n.to_string(), t.split('#').next().unwrap_or("").to_string()),
                    None => (riot.to_string(), first(node, &["riotIdTagLine", "tagLine"]).to_string()),
                };
                out.push(Player { puuid: puuid.into(), name, tag, role: role.into() });
            }
            for v in o.values() {
                if v.is_object() || v.is_array() {
                    collect_players(v, out, depth + 1);
                }
            }
        }
        _ => {}
    }
}

/// Meilleures ressemblances d'abord ; une personne et un joueur ne servent qu'une fois.
pub fn match_roles(i: &Inner, players: &[Player]) -> HashMap<String, String> {
    let mut pairs: Vec<(String, usize, f64)> = vec![];
    for m in &i.members {
        let mut names = vec![m.name.clone()];
        if let Some(c) = i.custom.get(&m.id) {
            if !c.name.is_empty() {
                names.push(c.name.clone());
            }
        }
        for (pi, p) in players.iter().enumerate() {
            let score = names.iter().map(|n| similarity(n, &p.name)).fold(0.0, f64::max);
            if score >= 0.75 {
                pairs.push((m.id.clone(), pi, score));
            }
        }
    }
    pairs.sort_by(|a, b| b.2.total_cmp(&a.2));
    let mut found = HashMap::new();
    let mut used = HashSet::new();
    for (id, pi, _) in pairs {
        if found.contains_key(&id) || used.contains(&pi) {
            continue;
        }
        found.insert(id, players[pi].role.clone());
        used.insert(pi);
    }
    found
}

struct Lock {
    port: u16,
    password: String,
}

/// Où chercher le « lockfile » du client League.
struct Finder {
    from_process: Option<PathBuf>,
    checked_at: Option<Instant>,
    sys: sysinfo::System,
}

impl Finder {
    fn dirs(&mut self) -> Vec<PathBuf> {
        let mut dirs = vec![];
        if let Some(d) = &self.from_process {
            dirs.push(d.clone());
        }
        if let Ok(yaml) = std::fs::read_to_string(r"C:\ProgramData\Riot Games\Metadata\league_of_legends.live\league_of_legends.live.product_settings.yaml") {
            for line in yaml.lines() {
                if let Some(rest) = line.trim().strip_prefix("product_install_full_path:") {
                    dirs.push(PathBuf::from(rest.trim().trim_matches('"')));
                }
            }
        }
        for d in "CDEFG".chars() {
            dirs.push(PathBuf::from(format!(r"{d}:\Riot Games\League of Legends")));
        }
        dirs
    }

    /// Dossier trouvé à partir du client League lancé (marche même s'il est installé ailleurs).
    fn refresh_from_process(&mut self) {
        if self.checked_at.is_some_and(|t| t.elapsed() < Duration::from_secs(30)) {
            return;
        }
        self.checked_at = Some(Instant::now());
        self.sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        self.from_process = self
            .sys
            .processes()
            .values()
            .find(|p| p.name().to_string_lossy().eq_ignore_ascii_case("LeagueClientUx.exe"))
            .and_then(|p| p.exe())
            .and_then(|e| e.parent())
            .map(PathBuf::from);
    }

    fn lockfile(&mut self) -> Option<Lock> {
        for pass in 0..2 {
            for dir in self.dirs() {
                let Ok(txt) = std::fs::read_to_string(dir.join("lockfile")) else { continue };
                let parts: Vec<&str> = txt.trim().split(':').collect();
                if parts.len() >= 4 {
                    if let Ok(port) = parts[2].parse() {
                        return Some(Lock { port, password: parts[3].to_string() });
                    }
                }
            }
            if pass == 0 {
                self.refresh_from_process();
            }
        }
        None
    }
}

/// Requête locale vers le client League (certificat auto-signé de Riot, uniquement sur 127.0.0.1).
async fn local_get(http: &reqwest::Client, port: u16, path: &str, auth: Option<&str>) -> Option<Value> {
    let mut req = http.get(format!("https://127.0.0.1:{port}{path}")).header("Accept", "application/json");
    if let Some(pw) = auth {
        req = req.basic_auth("riot", Some(pw));
    }
    let r = req.send().await.ok()?;
    if r.status() != 200 {
        return None;
    }
    let body = r.bytes().await.ok()?;
    if body.len() > 2_000_000 {
        return None;
    }
    serde_json::from_slice(&body).ok()
}

struct Reader {
    http: reqwest::Client,
    finder: Finder,
    names: HashMap<String, (String, String)>, // puuid -> (nom, tag)
}

impl Reader {
    async fn read(&mut self, app: &App) -> (&'static str, Option<RiotLink>, Vec<Player>) {
        let mut players = vec![];
        let mut me = None;
        // 1. En partie : l'API du jeu donne les 10 joueurs avec leur rôle.
        if let Some(live) = local_get(&self.http, 2999, "/liveclientdata/playerlist", None).await {
            if live.as_array().is_some_and(|a| !a.is_empty()) {
                collect_players(&live, &mut players, 0);
                return ("game", None, dedupe(players));
            }
        }
        // 2. Dans le client : salon de la partie, puis sélection des champions.
        let Some(lock) = self.finder.lockfile() else { return ("off", None, vec![]) };
        let pw = Some(lock.password.as_str());
        let (lobby, session) = tokio::join!(
            local_get(&self.http, lock.port, "/lol-lobby/v2/lobby", pw),
            local_get(&self.http, lock.port, "/lol-champ-select/v1/session", pw),
        );
        let status = if lobby.is_some() || session.is_some() { "on" } else { "off" };
        // Ton propre Riot ID : te relie directement à ton compte Discord.
        if status == "on" && app.read(|i| i.league.me.is_none()) {
            if let Some(v) = local_get(&self.http, lock.port, "/lol-summoner/v1/current-summoner", pw).await {
                let name = s(&v, "gameName");
                if !name.is_empty() {
                    me = Some(RiotLink { name: name.into(), tag: s(&v, "tagLine").into() });
                }
            }
        }
        for v in [&lobby, &session].into_iter().flatten() {
            collect_players(v, &mut players, 0);
        }
        // Noms manquants : on les demande au client à partir du puuid.
        for p in players.iter_mut().filter(|p| p.name.is_empty() && !p.puuid.is_empty()) {
            if !self.names.contains_key(&p.puuid) {
                let path = format!("/lol-summoner/v2/summoners/puuid/{}", p.puuid.replace(|c: char| !c.is_ascii_alphanumeric() && c != '-', ""));
                let v = local_get(&self.http, lock.port, &path, pw).await;
                let n = v.map(|v| (first(&v, &["gameName", "displayName"]).to_string(), s(&v, "tagLine").to_string())).unwrap_or_default();
                self.names.insert(p.puuid.clone(), n);
            }
            let (n, t) = self.names[&p.puuid].clone();
            p.name = n;
            p.tag = t;
        }
        if std::env::var_os("VO_DEBUG").is_some() {
            log::debug!("League : {players:?}");
        }
        (status, me, dedupe(players))
    }
}

/// Un même joueur peut apparaître deux fois (salon + sélection) : on garde la dernière info.
fn dedupe(players: Vec<Player>) -> Vec<Player> {
    let mut out: Vec<Player> = vec![];
    for p in players.into_iter().filter(|p| !p.name.is_empty()) {
        let key = if p.puuid.is_empty() { p.name.to_lowercase() } else { p.puuid.clone() };
        let same = |x: &Player| (if x.puuid.is_empty() { x.name.to_lowercase() } else { x.puuid.clone() }) == key;
        match out.iter_mut().find(|x| same(x)) {
            Some(x) => *x = p,
            None => out.push(p),
        }
    }
    out
}

/// Une lecture à la fois : la suivante part 3 s après la fin de la précédente (jamais de chevauchement).
pub async fn run(app: Arc<App>) {
    let http = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_millis(2500))
        .no_proxy()
        .build()
        .expect("client HTTP");
    let mut reader = Reader { http, finder: Finder { from_process: None, checked_at: None, sys: sysinfo::System::new() }, names: HashMap::new() };
    loop {
        let (status, me, players) = reader.read(&app).await;
        app.update(|i| {
            let status_changed = i.league.status != status;
            i.league.status = status;
            if me.is_some() {
                i.league.me = me;
            }
            if status == "on" {
                i.link_self_from_league();
            }
            let roles = match_roles(i, &players);
            let changed = roles != i.league.roles;
            // Hors partie et hors salon, on garde les derniers rôles connus (le temps de rejoindre la game).
            if !players.is_empty() {
                i.league.roles = roles;
            }
            if (!players.is_empty() && changed) || status_changed {
                i.broadcast();
            }
        });
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests::{inner, member};
    use serde_json::json;

    #[test]
    fn collects_from_nested_formats() {
        let v = json!({
            "localMember": { "gameName": "Caillou", "tagLine": "EUW", "firstPositionPreference": "MIDDLE", "puuid": "p1" },
            "members": [ { "riotId": "Bally#777", "assignedPosition": "utility" }, { "summonerName": "NoRole" } ]
        });
        let mut out = vec![];
        collect_players(&v, &mut out, 0);
        assert!(out.contains(&Player { puuid: "p1".into(), name: "Caillou".into(), tag: "EUW".into(), role: "mid".into() }));
        assert!(out.contains(&Player { puuid: "".into(), name: "Bally".into(), tag: "777".into(), role: "support".into() }));
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn matches_each_player_once() {
        let mut i = inner();
        i.upsert_member(member("30001", "Cailloux"));
        i.upsert_member(member("30002", "Caillou"));
        let players = vec![Player { puuid: "".into(), name: "Caillou".into(), tag: "".into(), role: "top".into() }];
        let roles = match_roles(&i, &players);
        assert_eq!(roles.len(), 1);
        assert_eq!(roles["30002"], "top", "le plus ressemblant gagne");
    }

    #[test]
    fn dedupe_keeps_last() {
        let p = |r: &str| Player { puuid: "x".into(), name: "A".into(), tag: "".into(), role: r.into() };
        assert_eq!(dedupe(vec![p("top"), p("mid")]), vec![p("mid")]);
    }
}
