//! Pont DPM (inhouse.dpm.lol).
//!
//! L'extension Chrome « Voice Overlay – pont DPM » envoie le lobby tel que ta page DPM le reçoit :
//! pour chaque joueur, son Riot ID, son équipe, son rôle et « dans le vocal : oui/non ».
//! DPM ne donne pas les comptes Discord : on les retrouve en associant chaque arrivée dans le vocal
//! Discord à la pastille qui passe au vert au même moment. Chaque lien trouvé est gardé pour toujours.

use crate::settings::RiotLink;
use crate::similarity::similarity;
use crate::state::{DpmPlayer, Inner, Lobby};
use crate::util::{now_ms, truncate};
use serde_json::{json, Value};
use std::collections::HashSet;

pub const LOBBY_MAX_AGE: i64 = 3 * 3600 * 1000;
const WINDOW: i64 = 20_000;
const CLOSE: i64 = 1_500;

pub fn block_key(id: &str, key: &str) -> String {
    format!("{id}|{key}")
}
pub fn riot_key(name: &str, tag: &str) -> String {
    format!("{}#{}", name.to_lowercase(), tag.to_lowercase())
}
fn link_key(l: &RiotLink) -> String {
    riot_key(&l.name, &l.tag)
}

fn lane(v: &str) -> &'static str {
    match v.to_lowercase().as_str() {
        "top" => "top",
        "jungle" => "jungle",
        "mid" | "middle" => "mid",
        "adc" | "bottom" | "bot" => "adc",
        "support" | "utility" => "support",
        _ => "",
    }
}

fn str_of<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

impl Inner {
    fn live_lobby(&self) -> Option<&Lobby> {
        self.dpm.lobby.as_ref().filter(|l| now_ms() - l.at < LOBBY_MAX_AGE)
    }

    fn dpm_player_of(&self, id: &str) -> Option<&DpmPlayer> {
        let l = self.dpm.links.get(id)?;
        let key = link_key(l);
        self.live_lobby()?.players.iter().find(|p| p.key == key)
    }
    pub fn dpm_role(&self, id: &str) -> String {
        self.dpm_player_of(id).map(|p| p.lane.clone()).unwrap_or_default()
    }
    pub fn dpm_side(&self, id: &str) -> String {
        self.dpm_player_of(id).map(|p| p.side.clone()).unwrap_or_default()
    }

    /// L'extension (pont DPM) a donné des nouvelles récemment. Elle envoie un signal chaque minute.
    pub fn dpm_extension_connected(&self) -> bool {
        now_ms() - self.dpm.last_extension.max(self.dpm.last_ping) < 150_000
    }

    /// Signal de l'extension : prévient les pages tout de suite si elle vient d'apparaître.
    pub fn dpm_ping(&mut self) {
        self.dpm.last_ping = now_ms();
        if !self.dpm.ext_shown {
            self.dpm.ext_shown = true;
            self.broadcast();
        }
    }

    pub fn dpm_status(&self) -> Value {
        let lobby = self.live_lobby().map(|l| {
            let linked: HashSet<String> = self.dpm.links.values().map(link_key).collect();
            json!({
                "state": l.state,
                "players": l.players.len(),
                "inVoice": l.players.iter().filter(|p| p.in_voice).count(),
                "linked": l.players.iter().filter(|p| linked.contains(&p.key)).count(),
            })
        });
        json!({
            "extension": self.dpm_extension_connected(),
            "lobby": lobby,
            "links": self.dpm.links.len(),
        })
    }

    /// Équipe du vocal suivi : ton équipe d'après DPM, sinon le nom du salon, sinon les personnes déjà reliées.
    fn dpm_voice_side(&self) -> String {
        if let Some(l) = &self.dpm.lobby {
            if !l.you_side.is_empty() {
                return l.you_side.clone();
            }
        }
        let name = self.channel.as_ref().map(|c| c.name.to_lowercase()).unwrap_or_default();
        let has_word = |w: &str| name.split(|c: char| !c.is_alphanumeric()).any(|x| x == w);
        if has_word("red") || has_word("rouge") {
            return "red".into();
        }
        if has_word("blue") || has_word("bleu") {
            return "blue".into();
        }
        let sides: Vec<String> = self.members.iter().map(|m| self.dpm_side(&m.id)).filter(|s| !s.is_empty()).collect();
        if !sides.is_empty() && sides.iter().all(|s| *s == sides[0]) {
            return sides[0].clone();
        }
        String::new()
    }

    /// Lobby reçu de l'extension.
    pub fn dpm_update(&mut self, b: &Value) {
        let Some(list) = b.get("players").and_then(Value::as_array) else { return };
        let now = now_ms();
        self.dpm.last_extension = now;
        let players: Vec<DpmPlayer> = list
            .iter()
            .take(20)
            .map(|p| {
                let side = match str_of(p, "side") {
                    s @ ("red" | "blue") => s.to_string(),
                    _ => String::new(),
                };
                let l = Some(str_of(p, "lane")).filter(|s| !s.is_empty()).unwrap_or(str_of(p, "chosenRole"));
                let name = truncate(str_of(p, "gameName"), 40);
                let tag = truncate(str_of(p, "tagLine"), 10);
                DpmPlayer { key: riot_key(&name, &tag), name, tag, side, lane: lane(l).into(), in_voice: p.get("inVoice").and_then(Value::as_bool).unwrap_or(false) }
            })
            .filter(|p| !p.name.is_empty())
            .collect();
        let match_id = truncate(&match b.get("matchId") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(n)) => n.to_string(),
            _ => String::new(),
        }, 64);
        let is_new = self.dpm.lobby.as_ref().is_none_or(|l| l.match_id != match_id);
        if is_new {
            self.dpm.greens.clear();
            self.dpm.joins.clear();
            self.dpm.blocked.clear();
        } else if let Some(old) = &self.dpm.lobby {
            // Pastille qui passe au vert (seulement les changements vus en direct, pas l'état de départ)
            for p in &players {
                if p.in_voice && old.players.iter().any(|o| o.key == p.key && !o.in_voice) {
                    self.dpm.greens.push((p.key.clone(), now));
                }
            }
        }
        let you_side = match b.get("you").map(|y| str_of(y, "side")) {
            Some(s @ ("blue" | "red")) => s.to_string(),
            _ => String::new(),
        };
        if is_new {
            log::info!("Lobby DPM : {} joueurs ({} dans le vocal).", players.len(), players.iter().filter(|p| p.in_voice).count());
        }
        self.dpm.lobby = Some(Lobby { match_id, state: truncate(str_of(b, "state"), 30), players, you_side, at: now });
        self.dpm_correlate();
        self.broadcast();
    }

    /// Quelqu'un vient d'arriver dans le vocal.
    pub fn dpm_join(&mut self, id: &str) {
        if self.dpm.lobby.as_ref().is_none_or(|l| now_ms() - l.at > 15 * 60 * 1000) {
            return;
        }
        self.dpm.joins.push((id.to_string(), now_ms()));
    }

    fn dpm_link(&mut self, id: &str, p: &DpmPlayer, how: &str) -> bool {
        if self.dpm.blocked.contains(&block_key(id, &p.key)) {
            return false;
        }
        // Un joueur League ne va qu'avec une seule personne Discord.
        self.dpm.links.retain(|other, l| other == id || link_key(l) != p.key);
        self.dpm.links.insert(id.to_string(), RiotLink { name: p.name.clone(), tag: p.tag.clone() });
        let who = self.member(id).map(|m| m.name.clone()).unwrap_or_else(|| id.to_string());
        log::info!("Lien DPM ({how}) : {who} ↔ {}#{}", p.name, p.tag);
        self.save_people_soon();
        true
    }

    /// Oublie un lien (ou tous) ; il n'est pas recréé avant le prochain lobby.
    pub fn dpm_forget(&mut self, id: Option<&str>) {
        let ids: Vec<String> = match id {
            Some(id) => vec![id.to_string()],
            None => self.dpm.links.keys().cloned().collect(),
        };
        for id in ids {
            if let Some(l) = self.dpm.links.remove(&id) {
                self.dpm.blocked.insert(block_key(&id, &link_key(&l)));
            }
        }
        self.save_people_soon();
        self.broadcast();
    }

    /// Le client League donne ton propre Riot ID : il te relie directement à ton compte Discord.
    pub fn link_self_from_league(&mut self) {
        let (Some(me), Some(self_id)) = (self.league.me.clone(), self.self_id.clone()) else { return };
        if self.dpm.links.contains_key(&self_id) || self.dpm.blocked.contains(&block_key(&self_id, &link_key(&me))) {
            return;
        }
        log::info!("Lien (client League) : toi ↔ {}#{}", me.name, me.tag);
        self.dpm.links.insert(self_id, me);
        self.save_people_soon();
    }

    fn links_fingerprint(&self) -> String {
        let mut v: Vec<String> = self.dpm.links.iter().map(|(k, l)| format!("{k}={}", link_key(l))).collect();
        v.sort();
        v.join(",")
    }

    /// On recommence tant qu'on trouve de nouveaux liens : un lien trouvé par pseudo peut en débloquer un autre par élimination.
    pub fn dpm_correlate(&mut self) {
        for _ in 0..6 {
            let before = self.links_fingerprint();
            self.dpm_correlate_once();
            if self.links_fingerprint() == before {
                break;
            }
        }
    }

    fn dpm_correlate_once(&mut self) {
        let Some(lobby) = self.dpm.lobby.clone() else { return };
        let player = |key: &str| lobby.players.iter().find(|p| p.key == key).cloned();
        let linked_keys = |me: &Inner| -> HashSet<String> { me.dpm.links.values().map(link_key).collect() };
        let is_linked_id = |me: &Inner, id: &str| me.dpm.links.get(id).is_some_and(|l| lobby.players.iter().any(|p| p.key == link_key(l)));

        // 1. Par ordre d'arrivée : 1re arrivée Discord libre = 1re pastille verte libre, si c'est sans ambiguïté.
        for _ in 0..20 {
            let keys = linked_keys(self);
            let joins: Vec<(String, i64)> = self.dpm.joins.iter().filter(|(id, _)| !is_linked_id(self, id) && self.has_member(id)).cloned().collect();
            let greens: Vec<(String, i64)> = self.dpm.greens.iter().filter(|(k, _)| !keys.contains(k)).cloned().collect();
            let (Some(j), Some(g)) = (joins.first().cloned(), greens.first().cloned()) else { break };
            if (g.1 - j.1).abs() > WINDOW {
                // Trop éloignés : on écarte le plus ancien des deux, qui n'a pas de partenaire.
                if j.1 < g.1 { self.dpm.joins.retain(|x| *x != j) } else { self.dpm.greens.retain(|x| *x != g) }
                continue;
            }
            let near_j: Vec<&(String, i64)> = joins.iter().filter(|x| (x.1 - j.1).abs() <= CLOSE).collect();
            let near_g: Vec<&(String, i64)> = greens.iter().filter(|x| (x.1 - g.1).abs() <= CLOSE).collect();
            if near_j.len() == 1 && near_g.len() == 1 {
                let ok = player(&g.0).is_some_and(|p| self.dpm_link(&j.0, &p, "arrivée"));
                // Lien oublié à la main : cette arrivée ne compte plus.
                if !ok {
                    self.dpm.joins.retain(|x| *x != j);
                }
                continue;
            }
            // Arrivées presque simultanées : on départage par la ressemblance des pseudos, sinon on attend.
            let mut solved = false;
            for x in &near_j {
                let mname = self.member(&x.0).map(|m| m.name.clone()).unwrap_or_default();
                let mut scored: Vec<(DpmPlayer, f64)> = near_g.iter().filter_map(|y| player(&y.0)).map(|p| { let s = similarity(&mname, &p.name); (p, s) }).collect();
                scored.sort_by(|a, b| b.1.total_cmp(&a.1));
                if !scored.is_empty() && scored[0].1 >= 0.6 && (scored.len() == 1 || scored[0].1 - scored[1].1 >= 0.2) && self.dpm_link(&x.0, &scored[0].0.clone(), "arrivée + pseudo") {
                    solved = true;
                    break;
                }
            }
            if !solved {
                break;
            }
        }

        // 2. Par élimination : chaque équipe a son vocal. On ne compte que les joueurs de l'équipe de ce vocal.
        let side = self.dpm_voice_side();
        let keys = linked_keys(self);
        let pool: Vec<&DpmPlayer> = lobby.players.iter().filter(|p| p.in_voice && (side.is_empty() || p.side == side)).collect();
        let green_unlinked: Vec<&DpmPlayer> = pool.iter().copied().filter(|p| !keys.contains(&p.key)).collect();
        let voice_unlinked: Vec<String> = self.members.iter().map(|m| m.id.clone()).filter(|id| !is_linked_id(self, id)).collect();
        if green_unlinked.len() == 1 && voice_unlinked.len() == 1 && pool.len() == self.members.len() {
            let p = green_unlinked[0].clone();
            self.dpm_link(&voice_unlinked[0], &p, "élimination");
        }

        // 3. Pseudos qui se ressemblent nettement.
        let ids: Vec<String> = self.members.iter().map(|m| m.id.clone()).collect();
        for id in ids {
            if is_linked_id(self, &id) {
                continue;
            }
            let name = self.member(&id).map(|m| m.name.clone()).unwrap_or_default();
            let ks = linked_keys(self);
            let mut scored: Vec<(DpmPlayer, f64)> = lobby
                .players
                .iter()
                .filter(|p| !ks.contains(&p.key) && !self.dpm.blocked.contains(&block_key(&id, &p.key)))
                .map(|p| (p.clone(), similarity(&name, &p.name)))
                .collect();
            scored.sort_by(|a, b| b.1.total_cmp(&a.1));
            if !scored.is_empty() && scored[0].1 >= 0.85 && (scored.len() == 1 || scored[0].1 - scored[1].1 >= 0.15) {
                let p = scored[0].0.clone();
                self.dpm_link(&id, &p, "pseudo");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::state::tests::{inner, member};
    use serde_json::json;

    fn lobby(match_id: &str, players: &[(&str, &str, bool)]) -> serde_json::Value {
        json!({
            "matchId": match_id, "state": "lobby",
            "players": players.iter().map(|(n, side, v)| json!({ "gameName": n, "tagLine": "EUW", "side": side, "lane": "MIDDLE", "inVoice": v })).collect::<Vec<_>>()
        })
    }

    #[test]
    fn links_by_arrival_order() {
        let mut i = inner();
        i.dpm_update(&lobby("m1", &[("Xyzzy", "blue", false), ("Quux", "blue", false)]));
        // Arrivée Discord de « Bob » au moment où la pastille de « Xyzzy » passe au vert.
        i.upsert_member(member("20001", "Bob"));
        i.dpm_join("20001");
        i.dpm_update(&lobby("m1", &[("Xyzzy", "blue", true), ("Quux", "blue", false)]));
        assert_eq!(i.dpm.links["20001"].name, "Xyzzy");
        assert_eq!(i.dpm_role("20001"), "mid");
        assert_eq!(i.dpm_side("20001"), "blue");
    }

    #[test]
    fn links_by_similar_name() {
        let mut i = inner();
        i.upsert_member(member("20001", "Caillou"));
        i.dpm_update(&lobby("m1", &[("Caillou", "red", false), ("Other", "red", false)]));
        assert_eq!(i.dpm.links["20001"].name, "Caillou");
    }

    #[test]
    fn links_by_elimination_within_team() {
        let mut i = inner();
        i.channel = Some(crate::state::Channel { id: "1".into(), name: "Équipe Rouge".into(), guild_id: None });
        i.upsert_member(member("20001", "Caillou"));
        i.upsert_member(member("20002", "zzz"));
        i.dpm_update(&lobby("m1", &[("Caillou", "red", true), ("Qwerty", "red", true), ("Blue1", "blue", true)]));
        assert_eq!(i.dpm.links["20001"].name, "Caillou");
        assert_eq!(i.dpm.links["20002"].name, "Qwerty", "dernier joueur rouge non relié");
    }

    #[test]
    fn forgotten_link_is_not_recreated_until_next_lobby() {
        let mut i = inner();
        i.upsert_member(member("20001", "Caillou"));
        i.dpm_update(&lobby("m1", &[("Caillou", "red", false)]));
        assert!(i.dpm.links.contains_key("20001"));
        i.dpm_forget(Some("20001"));
        i.dpm_update(&lobby("m1", &[("Caillou", "red", false)]));
        assert!(!i.dpm.links.contains_key("20001"));
        i.dpm_update(&lobby("m2", &[("Caillou", "red", false)]));
        assert!(i.dpm.links.contains_key("20001"), "nouveau lobby : on peut relier à nouveau");
    }
}
