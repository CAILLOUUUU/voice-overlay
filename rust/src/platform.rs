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

/// Icône près de l'horloge : clic gauche = réglages, clic droit = menu (réglages / quitter).
/// Bloque le fil d'exécution appelant (boucle de messages Windows).
#[cfg(windows)]
pub fn run_tray(settings_url: String, fr: bool, on_quit: impl Fn() + Send + Sync + 'static) {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem};
    use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, TranslateMessage, MSG};

    let icon = match decode_icon() {
        Some(i) => i,
        None => return log::error!("Icône de notification indisponible : image illisible"),
    };
    let menu = Menu::new();
    let open_item = MenuItem::new(if fr { "Ouvrir les réglages" } else { "Open settings" }, true, None);
    let quit_item = MenuItem::new(if fr { "Quitter" } else { "Quit" }, true, None);
    if menu.append(&open_item).and_then(|_| menu.append(&quit_item)).is_err() {
        return log::error!("Icône de notification indisponible : menu");
    }
    let tray = TrayIconBuilder::new()
        .with_tooltip("Voice Overlay - Caillou feat Claude")
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build();
    let _tray = match tray {
        Ok(t) => t,
        Err(e) => return log::error!("Icône de notification indisponible : {e}"),
    };
    let (open_id, quit_id) = (open_item.id().clone(), quit_item.id().clone());
    let url = settings_url.clone();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        if e.id == open_id {
            open_url(&url);
        } else if e.id == quit_id {
            on_quit();
        }
    }));
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
            open_url(&settings_url);
        }
    }));
    // Boucle de messages : nécessaire pour que l'icône réponde.
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[cfg(windows)]
fn decode_icon() -> Option<tray_icon::Icon> {
    let png = crate::assets::get("icon.png")?;
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => buf[..info.buffer_size()].chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf[..info.buffer_size()].chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => buf[..info.buffer_size()].iter().flat_map(|&g| [g, g, g, 255]).collect(),
        _ => return None,
    };
    tray_icon::Icon::from_rgba(rgba, info.width, info.height).ok()
}
