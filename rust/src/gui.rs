//! Fenêtre des réglages et icône près de l'horloge (Windows, macOS).
//!
//! La fenêtre affiche la même page `settings.html` que le navigateur, avec le moteur web du système
//! (WebView2 sous Windows, WKWebView sous macOS) : l'aperçu reste exactement celui d'OBS.
//! Une seule fenêtre ; la fermer libère le moteur web (l'appli continue en arrière-plan).
//! Si la fenêtre ne peut pas être créée (WebView2 absent…), on ouvre la page dans le navigateur.
//!
//! Overlay en jeu : une deuxième fenêtre, transparente, toujours au-dessus, que la souris traverse,
//! exclue des captures (invisible dans OBS et les partages d'écran). Elle n'existe que pendant
//! une partie de League (ou pendant qu'on la place) ; elle ne prend jamais le focus du jeu.

use crate::platform::open_url;
use crate::state::App;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tao::dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use tao::window::{ResizeDirection, Window, WindowBuilder};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use wry::{NewWindowResponse, WebContext, WebView, WebViewBuilder};

pub enum UserEvent {
    ShowSettings,
    Quit,
    Tray(TrayIconEvent),
    Menu(MenuEvent),
    /// L'état de l'overlay en jeu a peut-être changé (partie, réglages, placement).
    Ingame,
    Hotkey(GlobalHotKeyEvent),
    /// Message de la page de l'overlay en jeu (« drag », « resize »)
    Ipc(String),
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

/// L'overlay en jeu. Même ordre des champs : le moteur web est libéré avant la fenêtre.
struct IngameWin {
    _webview: WebView,
    window: Window,
    _context: WebContext,
    /// Position/taille avec lesquelles la fenêtre a été ouverte (pour voir si on les a remises à zéro).
    rect: Option<[i32; 4]>,
}

/// Ce dont dépend l'overlay en jeu, lu dans l'état de l'appli.
#[derive(Clone, PartialEq)]
struct IngameWant {
    enabled: bool,
    show: bool,
    placing: bool,
    hotkey: String,
    rect: Option<[i32; 4]>,
}

fn ingame_want(app: &App) -> IngameWant {
    app.read(|i| IngameWant {
        enabled: i.ingame.enabled,
        show: i.ingame.enabled && (i.placing || i.league.status == "game"),
        placing: i.placing,
        hotkey: i.ingame.hotkey.clone(),
        rect: i.ingame.rect,
    })
}

/// La souris traverse la fenêtre (ou plus). On le fait nous-mêmes sous Windows :
/// la version de `tao` réaffiche la fenêtre en l'activant, ce qui enlèverait le focus au jeu.
#[cfg(windows)]
fn set_click_through(window: &Window, on: bool) {
    use tao::platform::windows::WindowExtWindows;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
        WS_EX_LAYERED, WS_EX_TRANSPARENT,
    };
    let hwnd = window.hwnd() as windows_sys::Win32::Foundation::HWND;
    let flags = (WS_EX_TRANSPARENT | WS_EX_LAYERED) as isize;
    // SAFETY : la fenêtre existe (on la tient) et ces appels ne font que changer son style.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let new = if on { ex | flags } else { ex & !flags };
        if new != ex {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new);
            SetWindowPos(hwnd, std::ptr::null_mut(), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
        }
    }
}

#[cfg(not(windows))]
fn set_click_through(window: &Window, on: bool) {
    let _ = window.set_ignore_cursor_events(on);
}

fn create_ingame(target: &EventLoopWindowTarget<UserEvent>, url: &str, port: u16, data: PathBuf, rect: Option<[i32; 4]>) -> Result<IngameWin, String> {
    let mut wb = WindowBuilder::new()
        .with_title("Voice Overlay - en jeu")
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top(true)
        .with_focused(false)
        .with_focusable(false)
        .with_resizable(true)
        .with_content_protection(true) // invisible dans OBS, les captures et les partages d'écran
        .with_min_inner_size(LogicalSize::new(80.0, 60.0));
    wb = match rect {
        Some([x, y, w, h]) => wb.with_position(PhysicalPosition::new(x, y)).with_inner_size(PhysicalSize::new(w.max(80) as u32, h.max(60) as u32)),
        None => wb.with_position(LogicalPosition::new(40.0, 160.0)).with_inner_size(LogicalSize::new(300.0, 420.0)),
    };
    #[cfg(windows)]
    {
        use tao::platform::windows::WindowBuilderExtWindows;
        wb = wb.with_skip_taskbar(true).with_undecorated_shadow(false);
    }
    let window = wb.build(target).map_err(|e| e.to_string())?;
    let mut context = WebContext::new(Some(data));
    let webview = WebViewBuilder::new_with_web_context(&mut context)
        .with_url(url)
        .with_transparent(true)
        .with_background_color((0, 0, 0, 0))
        .with_devtools(cfg!(debug_assertions))
        .with_focused(false)
        .with_ipc_handler(|req| {
            send(UserEvent::Ipc(req.body().clone()));
        })
        .with_navigation_handler(move |u| is_internal(&u, port))
        .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
        .build(&window)
        .map_err(|e| e.to_string())?;
    set_click_through(&window, true);
    Ok(IngameWin { _webview: webview, window, _context: context, rect })
}

/// Position et taille actuelles de la fenêtre, en pixels de l'écran.
fn window_rect(w: &Window) -> Option<[i32; 4]> {
    let p = w.outer_position().ok()?;
    let s = w.inner_size();
    Some([p.x, p.y, s.width as i32, s.height as i32])
}

/// Raccourci de l'overlay en jeu, réservé auprès du système.
struct Hotkeys {
    manager: Option<GlobalHotKeyManager>,
    current: Option<(String, HotKey)>,
}

impl Hotkeys {
    /// Réserve `want` (ou rien) ; renvoie `false` si le système l'a refusé.
    fn set(&mut self, want: Option<&str>) -> bool {
        if self.current.as_ref().map(|c| c.0.as_str()) == want {
            return true;
        }
        let Some(m) = &self.manager else { return want.is_none() };
        if let Some((_, h)) = self.current.take() {
            let _ = m.unregister(h);
        }
        let Some(w) = want else { return true };
        let Ok(h) = w.parse::<HotKey>() else { return false };
        match m.register(h) {
            Ok(()) => {
                self.current = Some((w.to_string(), h));
                true
            }
            Err(e) => {
                log::error!("Raccourci {w} indisponible : {e}");
                false
            }
        }
    }

    fn is(&self, id: u32) -> bool {
        self.current.as_ref().is_some_and(|c| c.1.id() == id)
    }
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
    let p2 = Mutex::new(proxy.clone());
    MenuEvent::set_event_handler(Some(Arc::new(move |e: MenuEvent| {
        if let Ok(p) = p2.lock() {
            let _ = p.send_event(UserEvent::Menu(e));
        }
    })));
    let p3 = Mutex::new(proxy.clone());
    GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
        if let Ok(p) = p3.lock() {
            let _ = p.send_event(UserEvent::Hotkey(e));
        }
    }));
    // L'overlay en jeu suit l'état de l'appli (partie en cours, réglages) : petit coup d'œil régulier.
    {
        let app = app.clone();
        std::thread::spawn(move || {
            let mut last: Option<IngameWant> = None;
            loop {
                let now = ingame_want(&app);
                if last.as_ref() != Some(&now) {
                    if proxy.send_event(UserEvent::Ingame).is_err() {
                        return;
                    }
                    last = Some(now);
                }
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
        });
    }
    let ingame_url = format!("http://localhost:{port}/?profile=ingame&ingame");

    let data = app.paths.dir.join("webview");
    let mut tray: Option<Tray> = None;
    let mut win: Option<Settings> = None;
    let mut game: Option<IngameWin> = None;
    let mut was_placing = false;
    let mut keys = Hotkeys { manager: None, current: None };

    event_loop.run(move |event, target, control_flow| {
        *control_flow = ControlFlow::Wait;
        let mut open = false;
        let mut quit = false;
        let mut sync = false;
        match event {
            // L'icône doit être créée une fois la boucle lancée (obligatoire sous macOS).
            Event::NewEvents(StartCause::Init) => {
                if with_tray {
                    tray = build_tray(fr);
                }
                keys.manager = GlobalHotKeyManager::new().map_err(|e| log::error!("Raccourcis clavier indisponibles : {e}")).ok();
                open = show_at_start;
            }
            Event::UserEvent(UserEvent::Hotkey(e)) if e.state == HotKeyState::Pressed && keys.is(e.id) => {
                // Le raccourci fait apparaître le cadre pour déplacer l'overlay, puis le valide.
                app.update(|i| {
                    i.placing = !i.placing && i.ingame.enabled;
                    i.broadcast();
                });
                sync = true;
            }
            Event::UserEvent(UserEvent::Ingame) => sync = true,
            Event::UserEvent(UserEvent::Ipc(m)) => {
                if let Some(g) = &game {
                    let _ = match m.as_str() {
                        "drag" => g.window.drag_window(),
                        "resize" => g.window.drag_resize_window(ResizeDirection::SouthEast),
                        _ => Ok(()),
                    };
                }
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
        if sync {
            let w = ingame_want(&app);
            let ok = keys.set(w.enabled.then_some(w.hotkey.as_str()));
            app.update(|i| {
                if i.hotkey_ok != ok {
                    i.hotkey_ok = ok;
                    i.broadcast();
                }
            });
            // Fin du placement : on retient où la fenêtre a été mise.
            if was_placing && !w.placing {
                if let Some(g) = game.as_mut() {
                    if let Some(r) = window_rect(&g.window) {
                        g.rect = Some(r);
                        app.update(|i| i.ingame.rect = Some(r));
                        app.save_store();
                    }
                }
            }
            was_placing = w.placing;
            // Position remise à zéro depuis les réglages : on rouvre la fenêtre à sa place de départ.
            let w = ingame_want(&app);
            if game.as_ref().is_some_and(|g| g.rect != w.rect) {
                game = None;
            }
            if w.show && game.is_none() {
                match create_ingame(target, &ingame_url, port, data.clone(), w.rect) {
                    Ok(g) => game = Some(g),
                    Err(e) => log::error!("Overlay en jeu impossible : {e}"),
                }
            } else if !w.show {
                game = None;
            }
            if let Some(g) = &game {
                set_click_through(&g.window, !w.placing);
            }
        }
        if quit {
            win = None;
            game = None;
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
