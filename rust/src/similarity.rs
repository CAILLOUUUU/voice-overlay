//! Ressemblance entre un pseudo Discord et un pseudo League.

use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

fn strip_accents(s: &str) -> String {
    s.nfd().filter(|c| !is_combining_mark(*c)).collect()
}

/// Minuscules, sans accents, seulement lettres et chiffres.
pub fn norm(s: &str) -> String {
    strip_accents(s).to_lowercase().chars().filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit()).collect()
}

/// Mots trop courants dans les pseudos pour servir à reconnaître quelqu'un.
const COMMON_WORDS: [&str; 18] = [
    "the", "les", "des", "and", "lol", "euw", "ttv", "twitch", "top", "mid", "adc", "sup", "jgl", "bot", "pro", "xxx", "officiel", "official",
];

/// Insère une espace entre deux caractères consécutifs qui vérifient `split`, en avançant
/// comme une expression régulière globale (une paire trouvée n'est pas réutilisée).
fn split_pairs(src: &[char], split: impl Fn(char, char) -> bool) -> Vec<char> {
    let mut out = Vec::with_capacity(src.len() * 2);
    let mut i = 0;
    while i < src.len() {
        if i + 1 < src.len() && split(src[i], src[i + 1]) {
            out.extend([src[i], ' ', src[i + 1]]);
            i += 2;
        } else {
            out.push(src[i]);
            i += 1;
        }
    }
    out
}

/// Découpe un pseudo en mots : espaces, ponctuation, chiffres et majuscules séparent les mots.
/// Même résultat que la version JavaScript (mêmes règles de découpe).
pub fn tokens(s: &str) -> Vec<String> {
    let src: Vec<char> = strip_accents(s).chars().collect();
    let camel = split_pairs(&src, |a, b| a.is_ascii_lowercase() && b.is_ascii_uppercase());
    let digits = split_pairs(&camel, |a, b| (a.is_ascii_alphabetic() && b.is_ascii_digit()) || (a.is_ascii_digit() && b.is_ascii_alphabetic()));
    let spaced: String = digits.into_iter().collect();
    spaced
        .to_lowercase()
        .split(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit()))
        .filter(|w| w.len() >= 3 && !w.bytes().all(|b| b.is_ascii_digit()) && !COMMON_WORDS.contains(w))
        .map(String::from)
        .collect()
}

/// Un mot du pseudo Discord identique à un mot du pseudo League (« Bally77 » ↔ « Bally Bagayoko »).
fn token_score(a: &str, b: &str) -> f64 {
    let (ta, tb) = (tokens(a), tokens(b));
    let mut best: f64 = 0.0;
    for x in &ta {
        for y in &tb {
            if x == y {
                best = best.max(if x.len() >= 4 { 0.85 } else { 0.8 });
            } else if x.len().min(y.len()) >= 4 && (x.starts_with(y.as_str()) || y.starts_with(x.as_str())) {
                best = best.max(0.78);
            }
        }
    }
    best
}

fn levenshtein(a: &[u8], b: &[u8]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

fn raw_similarity(a: &str, b: &str) -> f64 {
    let (a, b) = (norm(a), norm(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    if a.len().min(b.len()) >= 3 && (a.contains(&b) || b.contains(&a)) {
        return 0.9;
    }
    1.0 - levenshtein(a.as_bytes(), b.as_bytes()) as f64 / a.len().max(b.len()) as f64
}

fn no_digits(s: &str) -> String {
    norm(s).chars().filter(|c| !c.is_ascii_digit()).collect()
}

/// Score entre 0 (rien à voir) et 1 (identiques).
pub fn similarity(a: &str, b: &str) -> f64 {
    raw_similarity(a, b).max(raw_similarity(&no_digits(a), &no_digits(b))).max(token_score(a, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_and_accents() {
        assert_eq!(similarity("Caillou", "caillou"), 1.0);
        assert_eq!(similarity("Élodie", "elodie"), 1.0);
    }

    #[test]
    fn substring_and_digits() {
        assert_eq!(similarity("Caillou77", "Caillou"), 1.0, "chiffres ignorés");
        assert_eq!(similarity("Zed2000", "Zed1999"), 1.0, "chiffres ignorés");
    }

    #[test]
    fn shared_word() {
        assert!(similarity("Bally77", "Bally Bagayoko") >= 0.85);
        assert_eq!(tokens("ImTheBestADC99"), vec!["best"]);
    }

    #[test]
    fn unrelated() {
        assert!(similarity("Zed main", "Support Gap") < 0.5);
        assert_eq!(similarity("", "abc"), 0.0);
    }

    /// Valeurs calculées avec la fonction JavaScript d'origine (server.js).
    #[test]
    fn same_scores_as_javascript() {
        let cases = [
            ("Caillou77", "Caillou", 1.0), ("Zed2000", "Zed1999", 1.0), ("Bally77", "Bally Bagayoko", 0.9),
            ("Zed main", "Support Gap", 0.0), ("TTV mid", "TTV top", 0.5), ("ImTheBestADC99", "best", 0.9),
            ("xXDarkKnightXx", "DarkKnight", 0.9), ("Élodie", "elodie", 1.0), ("Jungle Diff", "JungleDiff", 1.0),
            ("mid or feed", "MidOrFeed", 1.0), ("Kiko", "Kikoo", 0.9), ("Rémi le Boss", "RemiBoss", 0.85),
            ("a1b2c3", "abc", 1.0), ("Toplaner", "Top Laner 2", 1.0),
        ];
        for (a, b, want) in cases {
            assert!((similarity(a, b) - want).abs() < 1e-6, "{a} / {b} : {} au lieu de {want}", similarity(a, b));
        }
        assert_eq!(tokens("a1b2c3dd4"), vec!["3dd"]);
    }

    #[test]
    fn common_words_do_not_match() {
        assert!(similarity("TTV mid", "TTV top") < 0.75);
    }
}
