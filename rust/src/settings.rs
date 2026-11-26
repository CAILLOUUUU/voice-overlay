//! Réglages de l'overlay (profils) et données par personne gardées sur le disque.

use crate::util::{is_discord_id, truncate};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// Rôles League of Legends, dans l'ordre d'affichage.
pub const ROLES: [&str; 5] = ["top", "jungle", "mid", "adc", "support"];
/// Délai avant d'oublier les renommages d'une personne partie (minutes ; -1 = jamais).
pub const FORGET_CHOICES: [i64; 5] = [0, 2, 10, 60, -1];

pub fn defaults() -> Map<String, Value> {
    let v = json!({
        "showDecorations": true,
        "showRoles": true,
        "showTitle": true,
        "titleText": "{salon}",
        "layout": "column",
        "align": "left",
        "size": 40,
        "gap": 6,
        "shape": "circle",
        "showNames": true,
        "showIcons": true,
        "hideSelf": false,
        "speakingOnly": false,
        "dimSilent": false,
        "effect": "glow",
        "animate": true,
        "speakColor": "#3ba55d",
        "accentColor": "#8b6cff",
        "textColor": "#ffffff",
        "bgColor": "#121318",
        "bgOpacity": 86,
        "font": "Inter",
        "bold": true,
        "customCss": ""
    });
    match v {
        Value::Object(m) => m,
        _ => unreachable!(),
    }
}

/// Même conversion que `Number(v)` en JavaScript (sauf cas exotiques).
fn js_number(v: &Value) -> Option<f64> {
    let n = match v {
        Value::Null => 0.0,
        Value::Bool(b) => f64::from(u8::from(*b)),
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() { 0.0 } else { t.parse().ok()? }
        }
        _ => return None,
    };
    n.is_finite().then_some(n)
}

fn num_value(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 9e15 { json!(n as i64) } else { json!(n) }
}

fn js_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

fn js_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "null".into(),
        Value::Array(_) | Value::Object(_) => "[object Object]".into(),
        other => other.to_string(),
    }
}

/// Garde seulement les réglages connus, avec le bon type ; le reste vient de `base`.
pub fn clean_settings(input: Option<&Value>, base: &Map<String, Value>) -> Map<String, Value> {
    let mut out = base.clone();
    let Some(Value::Object(input)) = input else { return out };
    for (k, d) in defaults() {
        let Some(v) = input.get(&k) else { continue };
        let nv = match d {
            Value::Bool(_) => Value::Bool(js_truthy(v)),
            Value::Number(_) => match js_number(v) {
                Some(n) => num_value(n),
                None => continue,
            },
            _ => Value::String(truncate(&js_string(v), if k == "customCss" { 20000 } else { 200 })),
        };
        out.insert(k, nv);
    }
    out
}

pub fn clean_name(n: &str) -> String {
    truncate(&n.split_whitespace().collect::<Vec<_>>().join(" "), 40)
}

/// Identifiant de profil à partir du nom : minuscules, sans accents, tirets.
pub fn profile_id_base(name: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let s: String = clean_name(name).nfd().filter(|c| !unicode_normalization::char::is_combining_mark(*c)).collect::<String>().to_lowercase();
    let mut out = String::new();
    let mut dash = false;
    for c in s.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
    }
    let t: String = out.trim_matches('-').chars().take(24).collect();
    if t.is_empty() { "profil".into() } else { t }
}

pub fn valid_profile_id(id: &str) -> bool {
    (1..=30).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[derive(Clone, Debug, Serialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub settings: Map<String, Value>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Custom {
    pub name: String,
    pub hidden: bool,
    pub role: String,
}

impl Custom {
    pub fn is_empty(&self) -> bool {
        self.name.is_empty() && !self.hidden && self.role.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RiotLink {
    pub name: String,
    pub tag: String,
}

/// Contenu de settings.json une fois relu et vérifié.
pub struct Loaded {
    pub lang: String,
    pub profiles: Vec<Profile>,
    pub forget_after: i64,
    pub custom: HashMap<String, Custom>,
    pub order: Vec<String>,
    pub last_seen: HashMap<String, i64>,
    pub links: HashMap<String, RiotLink>,
}

pub fn role_or_empty(v: Option<&Value>) -> String {
    match v.and_then(Value::as_str) {
        Some(r) if ROLES.contains(&r) => r.to_string(),
        _ => String::new(),
    }
}

pub fn load(raw: Option<Value>) -> Loaded {
    let raw = raw.unwrap_or(Value::Null);
    let lang = match raw.get("lang").and_then(Value::as_str) {
        Some(l @ ("fr" | "en")) => l.to_string(),
        _ => "auto".into(),
    };
    let defs = defaults();
    let mut profiles: Vec<Profile> = Vec::new();
    if let Some(Value::Array(ps)) = raw.get("profiles") {
        for p in ps {
            let Some(id) = p.get("id").and_then(Value::as_str) else { continue };
            if !valid_profile_id(id) || profiles.iter().any(|x| x.id == id) {
                continue;
            }
            let name = clean_name(p.get("name").and_then(Value::as_str).unwrap_or(""));
            profiles.push(Profile {
                id: id.into(),
                name: if name.is_empty() { id.into() } else { name },
                settings: clean_settings(p.get("settings"), &defs),
            });
        }
    }
    // Version 1.0.x : un seul style, rangé à plat dans settings.json
    if profiles.is_empty() {
        profiles.push(Profile { id: "principal".into(), name: "Principal".into(), settings: clean_settings(Some(&raw), &defs) });
    }
    let pp = raw.get("people").filter(|v| v.is_object()).cloned().unwrap_or(json!({}));
    let mut custom = HashMap::new();
    if let Some(Value::Object(c)) = pp.get("custom") {
        for (id, c) in c {
            if !is_discord_id(id) || !c.is_object() {
                continue;
            }
            let v = Custom {
                name: truncate(c.get("name").and_then(Value::as_str).unwrap_or("").trim(), 40),
                hidden: c.get("hidden").is_some_and(js_truthy),
                role: role_or_empty(c.get("role")),
            };
            if !v.is_empty() {
                custom.insert(id.clone(), v);
            }
        }
    }
    let forget_after = pp.get("forgetAfter").and_then(Value::as_i64).filter(|n| FORGET_CHOICES.contains(n)).unwrap_or(10);
    let order = match pp.get("order") {
        Some(Value::Array(a)) => a.iter().map(js_string).filter(|id| is_discord_id(id)).take(200).collect(),
        _ => vec![],
    };
    let mut last_seen = HashMap::new();
    if let Some(Value::Object(o)) = pp.get("lastSeen") {
        for (id, t) in o {
            if let (true, Some(t)) = (is_discord_id(id), t.as_f64()) {
                last_seen.insert(id.clone(), t as i64);
            }
        }
    }
    // Liens Discord ↔ Riot appris pendant les lobbies DPM : gardés pour toujours.
    let mut links = HashMap::new();
    if let Some(Value::Object(o)) = pp.get("links") {
        for (id, l) in o {
            let Some(name) = l.get("name").and_then(Value::as_str) else { continue };
            if !is_discord_id(id) {
                continue;
            }
            let tag = match l.get("tag") {
                Some(v) if js_truthy(v) => js_string(v),
                _ => String::new(),
            };
            links.insert(id.clone(), RiotLink { name: truncate(name, 40), tag: truncate(&tag, 10) });
        }
    }
    Loaded { lang, profiles, forget_after, custom, order, last_seen, links }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_keeps_types() {
        let base = defaults();
        let s = clean_settings(Some(&json!({ "size": "48", "animate": 0, "font": 12, "evil": "x", "gap": "abc" })), &base);
        assert_eq!(s["size"], json!(48));
        assert_eq!(s["animate"], json!(false));
        assert_eq!(s["font"], json!("12"));
        assert_eq!(s["gap"], json!(6), "valeur invalide : on garde la précédente");
        assert!(!s.contains_key("evil"));
    }

    #[test]
    fn profile_ids() {
        assert_eq!(profile_id_base("  Équipe   Rouge! "), "equipe-rouge");
        assert_eq!(profile_id_base("!!!"), "profil");
    }

    #[test]
    fn load_old_flat_format() {
        let l = load(Some(json!({ "size": 60, "lang": "fr" })));
        assert_eq!(l.lang, "fr");
        assert_eq!(l.profiles.len(), 1);
        assert_eq!(l.profiles[0].id, "principal");
        assert_eq!(l.profiles[0].settings["size"], json!(60));
        assert_eq!(l.forget_after, 10);
    }

    #[test]
    fn load_people_filters_bad_ids() {
        let l = load(Some(json!({ "people": {
            "forgetAfter": 60,
            "custom": { "123456789": { "name": " Bob ", "role": "mid" }, "abc": { "name": "x" }, "1234567": {} },
            "order": ["123456789", "bad", 42],
            "links": { "123456789": { "name": "Bob", "tag": "EUW" } }
        }})));
        assert_eq!(l.forget_after, 60);
        assert_eq!(l.custom.len(), 1);
        assert_eq!(l.custom["123456789"], Custom { name: "Bob".into(), hidden: false, role: "mid".into() });
        assert_eq!(l.order, vec!["123456789".to_string()]);
        assert_eq!(l.links["123456789"].tag, "EUW");
    }
}
