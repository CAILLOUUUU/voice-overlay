//! Ce qui dépend du système : navigateur, dossier, lancement avec Windows, icône près de l'horloge.

use std::path::Path;

pub fn open_url(url: &str) {
    if let Err(e) = open::that_detached(url) {
        log::error!("Impossible d'ouvrir {url} : {e}");
    }
}

pub fn open_folder(p: &Path) {
    let _ = open::that_detached(p);
}

pub const CAN_AUTOSTART: bool = cfg!(windows);

/// Ferme l'appli. Avec l'interface (fenêtre + icône), c'est elle qui ferme : elle retire d'abord
/// l'icône près de l'horloge, sinon Windows laisse une icône « fantôme » jusqu'au passage de la souris.
pub fn exit_app() {
    #[cfg(any(windows, target_os = "macos"))]
    if crate::gui::quit() {
        // L'interface ferme l'appli dans un instant ; au cas où, on ferme quand même.
        std::thread::sleep(std::time::Duration::from_secs(3));
    }
    std::process::exit(0);
}

/// Ouvre les réglages : dans la fenêtre de l'appli si possible, sinon dans le navigateur.
pub fn show_settings(url: &str) {
    #[cfg(any(windows, target_os = "macos"))]
    if crate::gui::show_settings() {
        return;
    }
    open_url(url);
}

#[cfg(windows)]
mod win {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::RegKey;

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const RUN_NAME: &str = "VoiceOverlayCaillou";

    pub fn autostart() -> bool {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(RUN_KEY, KEY_READ)
            .and_then(|k| k.get_value::<String, _>(RUN_NAME))
            .is_ok()
    }

    pub fn set_autostart(on: bool) -> bool {
        let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_READ | KEY_WRITE) else { return false };
        if on {
            let Ok(exe) = std::env::current_exe() else { return false };
            key.set_value(RUN_NAME, &format!("\"{}\" --silent", exe.display())).is_ok()
        } else {
            key.delete_value(RUN_NAME).is_ok() || !autostart()
        }
    }
}

pub fn autostart() -> bool {
    #[cfg(windows)]
    return win::autostart();
    #[cfg(not(windows))]
    false
}

pub fn set_autostart(on: bool) -> bool {
    #[cfg(windows)]
    return win::set_autostart(on);
    #[cfg(not(windows))]
    {
        let _ = on;
        false
    }
}
