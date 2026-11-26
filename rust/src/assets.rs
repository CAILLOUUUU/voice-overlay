//! Fichiers embarqués dans le .exe : pages, icônes, polices.
//! VO_ASSETS_DIR=chemin : lit les fichiers sur le disque (pratique pour modifier les pages sans recompiler).

use include_dir::{include_dir, Dir};
use std::borrow::Cow;

static WEBFONTS: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../webfonts");

const FILES: [(&str, &[u8]); 5] = [
    ("overlay.html", include_bytes!("../../overlay.html")),
    ("settings.html", include_bytes!("../../settings.html")),
    ("icon.png", include_bytes!("../../icon.png")),
    ("icon.ico", include_bytes!("../../icon.ico")),
    ("LICENCES-TIERS.txt", include_bytes!("../../LICENCES-TIERS.txt")),
];

pub fn get(name: &str) -> Option<Cow<'static, [u8]>> {
    if let Some(dir) = std::env::var_os("VO_ASSETS_DIR") {
        if name.split('/').any(|p| p == ".." || p.is_empty()) {
            return None;
        }
        return std::fs::read(std::path::Path::new(&dir).join(name)).ok().map(Cow::Owned);
    }
    if let Some(font) = name.strip_prefix("webfonts/") {
        return WEBFONTS.get_file(font).map(|f| Cow::Borrowed(f.contents()));
    }
    FILES.iter().find(|(n, _)| *n == name).map(|(_, b)| Cow::Borrowed(*b))
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_files_exist() {
        assert!(super::get("overlay.html").is_some());
        assert!(super::get("webfonts/inter-500.woff2").is_some());
        assert!(super::get("nope").is_none());
    }
}
