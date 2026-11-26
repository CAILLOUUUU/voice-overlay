//! Fenêtre des réglages et icône près de l'horloge (Windows, macOS).
//!
//! La fenêtre affiche la même page `settings.html` que le navigateur, avec le moteur web du système
//! (WebView2 sous Windows, WKWebView sous macOS) : l'aperçu reste exactement celui d'OBS.
//! Une seule fenêtre ; la fermer libère le moteur web (l'appli continue en arrière-plan).
//! Si la fenêtre ne peut pas être créée (WebView2 absent…), on ouvre la page dans le navigateur.

use crate::platform::open_url;
use crate::state::App;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tao::dpi::LogicalSize;
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use tao::window::{Window, WindowBuilder};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use wry::{NewWindowResponse, WebContext, WebView, WebViewBuilder};

pub enum UserEvent {
    ShowSettings,
    Quit,
    Tray(TrayIconEvent),
    Menu(MenuEvent),
}

static PROXY: OnceLock<Mutex<EventLoopProxy<UserEvent>>> = OnceLock::new();
static WINDOW_OPEN: AtomicBool = AtomicBool::new(false);

fn send(e: UserEvent) -> bool {
    PROXY.get().and_then(|p| p.lock().ok().map(|p| p.send_event(e).is_ok())).unwrap_or(false)
}

/// Ouvre (ou ramène au premier plan) la fenêtre des réglages. `false` si l'interface ne tourne pas.
pub fn show_settings() -> bool {
    send(UserEvent::ShowSettings)
}

/// Demande la fermeture de l'appli (icône retirée proprement). `false` si l'interface ne tourne pas.
pub fn quit() -> bool {
    send(UserEvent::Quit)
}

pub fn window_open() -> bool {
    WINDOW_OPEN.load(Ordering::SeqCst)
}

/// Icône de l'appli en RGBA (pour la fenêtre et pour l'icône près de l'horloge).
fn icon_rgba() -> Option<(Vec<u8>, u32, u32)> {
    let png = crate::assets::get("icon.png")?;
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let b = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => b.to_vec(),
        png::ColorType::Rgb => b.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => b.chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => b.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        _ => return None,
    };
    Some((rgba, info.width, info.height))
}

/// La fenêtre ouverte. L'ordre des champs compte : le moteur web est libéré avant la fenêtre.
struct Settings {
    webview: WebView,
    window: Window,
    _context: WebContext,
}

struct Tray {
    _icon: TrayIcon,
    open: MenuId,
    quit: MenuId,
}

fn build_tray(fr: bool) -> Option<Tray> {
    let (rgba, w, h) = icon_rgba()?;
    let icon = tray_icon::Icon::from_rgba(rgba, w, h).ok()?;
    let menu = Menu::new();
    let open = MenuItem::new(if fr { "Ouvrir les réglages" } else { "Open settings" }, true, None);
    let quit = MenuItem::new(if fr { "Quitter" } else { "Quit" }, true, None);
    menu.append(&open).ok()?;
    menu.append(&quit).ok()?;
    let built = TrayIconBuilder::new()
        .with_tooltip("Voice Overlay - Caillou feat Claude")
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build();
    match built {
        Ok(t) => Some(Tray { _icon: t, open: open.id().clone(), quit: quit.id().clone() }),
        Err(e) => {
            log::error!("Icône de notification indisponible : {e}");
            None
        }
    }
}

/// Adresses que la fenêtre peut afficher elle-même ; le reste part dans le navigateur.
fn is_internal(url: &str, port: u16) -> bool {
    let ok = [format!("http://localhost:{port}/"), format!("http://127.0.0.1:{port}/")];
    ok.iter().any(|p| url.starts_with(p.as_str()))
        || url == format!("http://localhost:{port}")
        || url.starts_with("about:")
        || url.starts_with("blob:")
        || url.starts_with("data:")
}

fn external(url: &str) {
    if url.starts_with("https://") || url.starts_with("http://") {
        open_url(url);
    }
}

fn create_window(target: &EventLoopWindowTarget<UserEvent>, url: &str, port: u16, data: PathBuf) -> Result<Settings, String> {
    let mut wb = WindowBuilder::new()
        .with_title("Voice Overlay - Caillou feat Claude")
        .with_inner_size(LogicalSize::new(1280.0, 900.0))
        .with_min_inner_size(LogicalSize::new(420.0, 480.0));
    if let Some((rgba, w, h)) = icon_rgba() {
        if let Ok(i) = tao::window::Icon::from_rgba(rgba, w, h) {
            wb = wb.with_window_icon(Some(i));
        }
    }
    let window = wb.build(target).map_err(|e| e.to_string())?;
    // Données du moteur web (stockage local de la page) dans le dossier de l'appli, jamais à côté du .exe.
    let mut context = WebContext::new(Some(data));
    let webview = WebViewBuilder::new_with_web_context(&mut context)
        .with_url(url)
        .with_background_color((17, 18, 20, 255))
        .with_devtools(cfg!(debug_assertions))
        .with_clipboard(true)
        .with_navigation_handler(move |u| {
            if is_internal(&u, port) {
                true
            } else {
                external(&u);
                false
            }
        })
        .with_new_window_req_handler(|u, _| {
            external(&u);
            NewWindowResponse::Deny
        })
        .build(&window)
        .map_err(|e| e.to_string())?;
    Ok(Settings { webview, window, _context: context })
}

/// Boucle de l'interface, sur le fil principal. Ne revient jamais.
pub fn run(app: Arc<App>, url: String, port: u16, fr: bool, show_at_start: bool, with_tray: bool) -> ! {
    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    #[cfg(target_os = "macos")]
    {
        // Pas d'icône dans le Dock : l'appli vit en arrière-plan.
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = &mut event_loop;
    let proxy = event_loop.create_proxy();
    let _ = PROXY.set(Mutex::new(proxy.clone()));
    let p1 = Mutex::new(proxy.clone());
    TrayIconEvent::set_event_handler(Some(move |e| {
        if let Ok(p) = p1.lock() {
            let _ = p.send_event(UserEvent::Tray(e));
        }
    }));
    let p2 = Mutex::new(proxy);
    MenuEvent::set_event_handler(Some(Arc::new(move |e: MenuEvent| {
        if let Ok(p) = p2.lock() {
            let _ = p.send_event(UserEvent::Menu(e));
        }
    })));

    let data = app.paths.dir.join("webview");
    let mut tray: Option<Tray> = None;
    let mut win: Option<Settings> = None;

    event_loop.run(move |event, target, control_flow| {
        *control_flow = ControlFlow::Wait;
        let mut open = false;
        let mut quit = false;
        match event {
            // L'icône doit être créée une fois la boucle lancée (obligatoire sous macOS).
            Event::NewEvents(StartCause::Init) => {
                if with_tray {
                    tray = build_tray(fr);
                }
                open = show_at_start;
            }
            Event::UserEvent(UserEvent::ShowSettings) => open = true,
            Event::UserEvent(UserEvent::Quit) => quit = true,
            Event::UserEvent(UserEvent::Menu(e)) => {
                if let Some(t) = &tray {
                    open = e.id == t.open;
                    quit = e.id == t.quit;
                }
            }
            Event::UserEvent(UserEvent::Tray(TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. })) => open = true,
            // Fermer la fenêtre libère le moteur web ; l'overlay continue de tourner.
            Event::WindowEvent { event: WindowEvent::CloseRequested, window_id, .. } if win.as_ref().is_some_and(|s| s.window.id() == window_id) => {
                win = None;
                WINDOW_OPEN.store(false, Ordering::SeqCst);
            }
            _ => {}
        }
        if quit {
            win = None;
            tray = None; // retire l'icône avant de fermer (pas d'icône « fantôme »)
            app.save_store();
            std::process::exit(0);
        }
        if open {
            match &win {
                Some(s) => {
                    s.window.set_visible(true);
                    s.window.set_minimized(false);
                    s.window.set_focus();
                    let _ = s.webview.focus();
                }
                None => match create_window(target, &url, port, data.clone()) {
                    Ok(s) => {
                        s.window.set_focus();
                        win = Some(s);
                        WINDOW_OPEN.store(true, Ordering::SeqCst);
                    }
                    Err(e) => {
                        log::error!("Fenêtre des réglages impossible ({e}) : ouverture dans le navigateur.");
                        open_url(&url);
                    }
                },
            }
        }
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn internal_urls() {
        assert!(super::is_internal("http://localhost:47474/settings", 47474));
        assert!(super::is_internal("http://127.0.0.1:47474/?preview&profile=a", 47474));
        assert!(super::is_internal("blob:http://localhost:47474/abc", 47474));
        assert!(!super::is_internal("http://localhost:47475/", 47474));
        assert!(!super::is_internal("https://discord.com/developers/applications", 47474));
        assert!(!super::is_internal("http://localhost:47474.evil.com/", 47474));
    }

    #[test]
    fn icon_decodes() {
        let (rgba, w, h) = super::icon_rgba().unwrap();
        assert_eq!(rgba.len() as u32, w * h * 4);
    }
}
