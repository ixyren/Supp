mod clipboard;
mod external_url;
mod session_script;
mod updater;

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    webview::{NewWindowResponse, WebviewBuilder},
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl, WindowEvent,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

static SIDEBAR_ANIM_ID: AtomicU64 = AtomicU64::new(0);
static SIDEBAR_ANIM_ACTIVE: AtomicBool = AtomicBool::new(false);
static CURRENT_WIDTH_TENTHS: AtomicU32 = AtomicU32::new(800);
static TARGET_WIDTH_TENTHS: AtomicU32 = AtomicU32::new(800);
static SWITCH_ANIM_ID: AtomicU64 = AtomicU64::new(0);
static INTRO_ACTIVE: AtomicBool = AtomicBool::new(true);

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub close_to_tray: bool,
    pub start_minimized: bool,
    pub open_at_login: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            close_to_tray: true,
            start_minimized: false,
            open_at_login: false,
        }
    }
}

pub struct AppState {
    pub settings: Mutex<AppSettings>,
    pub active_session_id: Mutex<Option<String>>,
}

fn sanitize_session_id(session_id: &str) -> Result<&str, String> {
    let trimmed = session_id.trim();
    if trimmed.is_empty() || trimmed.len() > 128 {
        return Err("ID de sesión inválido".to_string());
    }
    if !trimmed
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("El ID de sesión contiene caracteres no permitidos".to_string());
    }
    Ok(trimmed)
}

fn clamp_sidebar_width(width: f64) -> f64 {
    if width.is_finite() {
        width.clamp(80.0, 280.0)
    } else {
        80.0
    }
}

fn is_allowed_avatar_payload(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with("data:image/") {
        return trimmed.len() <= 200_000;
    }
    if trimmed.len() > 4096 {
        return false;
    }
    let Ok(parsed) = tauri::Url::parse(trimmed) else {
        return false;
    };
    if parsed.scheme() != "https" {
        return false;
    }
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    if h == "static.whatsapp.net" || h.ends_with(".static.whatsapp.net") {
        return false;
    }
    external_url::is_whatsapp_host(&h) || h == "fbcdn.net" || h.ends_with(".fbcdn.net")
}

fn get_window_logical_size(window: &tauri::Window) -> Option<LogicalSize<f64>> {
    if window.is_minimized().unwrap_or(false) {
        return None;
    }
    let window_size = window.inner_size().ok()?;
    let scale_factor = window.scale_factor().ok()?;
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        return None;
    }
    let logical_size = window_size.to_logical::<f64>(scale_factor);
    if logical_size.width < 100.0 || logical_size.height < 100.0 {
        return None;
    }
    Some(logical_size)
}

fn get_config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Error resolviendo directorio: {}", e))?;
    if !app_dir.exists() {
        fs::create_dir_all(&app_dir).map_err(|e| format!("Error creando directorio: {}", e))?;
    }
    Ok(app_dir.join("config.json"))
}

fn load_settings(app: &AppHandle) -> AppSettings {
    if let Ok(config_path) = get_config_path(app) {
        if config_path.exists() {
            if let Ok(data) = fs::read_to_string(&config_path) {
                if let Ok(settings) = serde_json::from_str::<AppSettings>(&data) {
                    return settings;
                }
            }
        }
    }
    AppSettings::default()
}

fn persist_settings(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let config_path = get_config_path(app)?;
    let data = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Error serializando configuraciones: {}", e))?;
    fs::write(config_path, data).map_err(|e| format!("Error escribiendo config: {}", e))?;
    Ok(())
}

#[cfg(target_os = "windows")]
mod win32 {
    #[repr(C)]
    pub struct RECT {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    #[repr(C)]
    pub struct POINT {
        pub x: i32,
        pub y: i32,
    }

    pub const GW_CHILD: u32 = 5;
    pub const GW_HWNDNEXT: u32 = 2;
    pub const GWL_STYLE: i32 = -16;
    pub const WS_CLIPSIBLINGS: isize = 0x04000000;
    pub const WS_CLIPCHILDREN: isize = 0x02000000;
    pub const HWND_TOP: isize = 0;
    pub const SWP_NOSIZE: u32 = 0x0001;
    pub const SWP_NOMOVE: u32 = 0x0002;
    pub const SWP_NOACTIVATE: u32 = 0x0010;
    pub const FLAGS: u32 = SWP_NOSIZE | SWP_NOMOVE | SWP_NOACTIVATE;
    pub const SW_RESTORE: i32 = 9;
    pub const SW_SHOW: i32 = 5;

    extern "system" {
        pub fn GetWindow(hwnd: isize, u_cmd: u32) -> isize;
        pub fn GetWindowRect(hwnd: isize, lp_rect: *mut RECT) -> i32;
        pub fn IsWindowVisible(hwnd: isize) -> i32;
        pub fn ScreenToClient(hwnd: isize, lp_point: *mut POINT) -> i32;
        pub fn GetWindowLongPtrW(hwnd: isize, n_index: i32) -> isize;
        pub fn SetWindowLongPtrW(hwnd: isize, n_index: i32, new_long: isize) -> isize;
        pub fn SetWindowPos(
            hwnd: isize,
            hwnd_insert_after: isize,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            u_flags: u32,
        ) -> i32;
        pub fn ShowWindow(hwnd: isize, n_cmd_show: i32) -> i32;
        pub fn SetForegroundWindow(hwnd: isize) -> i32;
        pub fn BringWindowToTop(hwnd: isize) -> i32;
        pub fn IsIconic(hwnd: isize) -> i32;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Motor de animación nativo con sincronización de backpressure en el hilo principal
// (evita saturar la cola de mensajes Win32/WebView2 y corre a ~120fps reales sin lag).
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(target_os = "windows")]
mod frame {
    use std::time::Duration;

    #[link(name = "winmm")]
    extern "system" {
        fn timeBeginPeriod(u_period: u32) -> u32;
        fn timeEndPeriod(u_period: u32) -> u32;
    }

    pub fn begin() {
        unsafe {
            timeBeginPeriod(1);
        }
    }
    pub fn end() {
        unsafe {
            timeEndPeriod(1);
        }
    }
    pub fn wait_remaining(elapsed: Duration) {
        let budget = Duration::from_millis(8);
        if let Some(rem) = budget.checked_sub(elapsed) {
            if !rem.is_zero() {
                std::thread::sleep(rem);
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod frame {
    use std::time::Duration;

    pub fn begin() {}
    pub fn end() {}
    pub fn wait_remaining(elapsed: Duration) {
        let budget = Duration::from_millis(8);
        if let Some(rem) = budget.checked_sub(elapsed) {
            if !rem.is_zero() {
                std::thread::sleep(rem);
            }
        }
    }
}

/// Ejecuta un paso de animación en el hilo principal de la ventana y espera a que
/// termine antes de calcular el siguiente frame, garantizando cero acumulación (lag)
/// de eventos `put_Bounds` en la cola de mensajes de Windows.
fn run_frame_sync<F>(window: &tauri::Window, f: F)
where
    F: FnOnce() + Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::sync_channel::<()>(1);
    if window
        .run_on_main_thread(move || {
            f();
            let _ = tx.send(());
        })
        .is_ok()
    {
        let _ = rx.recv_timeout(std::time::Duration::from_millis(32));
    }
}

/// Curva cubic-bezier(x1, y1, x2, y2) idéntica a la de CSS.
fn cubic_bezier(x1: f64, y1: f64, x2: f64, y2: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let bx = |t: f64| {
        let u = 1.0 - t;
        3.0 * u * u * t * x1 + 3.0 * u * t * t * x2 + t * t * t
    };
    let by = |t: f64| {
        let u = 1.0 - t;
        3.0 * u * u * t * y1 + 3.0 * u * t * t * y2 + t * t * t
    };
    let dx = |t: f64| {
        let u = 1.0 - t;
        3.0 * u * u * x1 + 6.0 * u * t * (x2 - x1) + 3.0 * t * t * (1.0 - x2)
    };
    let mut t = x;
    for _ in 0..8 {
        let err = bx(t) - x;
        if err.abs() < 1e-6 {
            break;
        }
        let d = dx(t);
        if d.abs() < 1e-6 {
            break;
        }
        t -= err / d;
    }
    t = t.clamp(0.0, 1.0);
    if (bx(t) - x).abs() > 1e-4 {
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..30 {
            t = (lo + hi) / 2.0;
            if bx(t) < x {
                lo = t;
            } else {
                hi = t;
            }
        }
    }
    by(t)
}

// Curvas y duraciones compartidas (deben coincidir con las de React en App.jsx)
const EXPAND_CURVE: (f64, f64, f64, f64) = (0.16, 1.0, 0.3, 1.0);
const EXPAND_SECS: f64 = 0.22;
const COLLAPSE_CURVE: (f64, f64, f64, f64) = (0.2, 0.9, 0.3, 1.0);
const COLLAPSE_SECS: f64 = 0.16;
const SLIDE_CURVE: (f64, f64, f64, f64) = (0.16, 1.0, 0.3, 1.0);
const SLIDE_SECS: f64 = 0.24;

#[cfg(target_os = "windows")]
unsafe fn ensure_sidebar_on_top(window: &tauri::Window) {
    let hwnd = match window.hwnd() {
        Ok(h) => h.0 as isize,
        Err(_) => return,
    };
    if hwnd == 0 || win32::IsIconic(hwnd) != 0 {
        return;
    }

    let parent_style = win32::GetWindowLongPtrW(hwnd, win32::GWL_STYLE);
    if parent_style & win32::WS_CLIPCHILDREN == 0 {
        win32::SetWindowLongPtrW(hwnd, win32::GWL_STYLE, parent_style | win32::WS_CLIPCHILDREN);
    }

    let mut visible_children = Vec::new();
    let mut main_children: Vec<isize> = Vec::new();
    let mut curr = win32::GetWindow(hwnd, win32::GW_CHILD);

    while curr != 0 {
        let style = win32::GetWindowLongPtrW(curr, win32::GWL_STYLE);
        let new_style = style | win32::WS_CLIPSIBLINGS | win32::WS_CLIPCHILDREN;
        if style != new_style {
            win32::SetWindowLongPtrW(curr, win32::GWL_STYLE, new_style);
        }

        if win32::IsWindowVisible(curr) != 0 {
            let mut rect = win32::RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            if win32::GetWindowRect(curr, &mut rect) != 0 {
                let width = rect.right - rect.left;
                let height = rect.bottom - rect.top;
                if width >= 50 && height >= 50 {
                    visible_children.push(curr);
                    let mut pt = win32::POINT {
                        x: rect.left,
                        y: rect.top,
                    };
                    if win32::ScreenToClient(hwnd, &mut pt) != 0
                        && pt.x >= -30
                        && pt.x <= 30
                        && pt.y >= -30
                        && pt.y <= 30
                    {
                        main_children.push(curr);
                    }
                }
            }
        }

        curr = win32::GetWindow(curr, win32::GW_HWNDNEXT);
    }

    // Solo reordenar si el webview principal visible NO está ya en el tope del orden Z,
    // y sin tocar los HWNDs de las sesiones de WhatsApp para evitar parpadeos de composición DWM.
    let already_on_top = !main_children.is_empty()
        && visible_children
            .iter()
            .take(main_children.len())
            .eq(main_children.iter());
    if !already_on_top {
        for m in main_children.iter().rev() {
            win32::SetWindowPos(*m, win32::HWND_TOP, 0, 0, 0, 0, win32::FLAGS);
        }
    }
}

pub fn show_and_focus_main_window(app: &AppHandle) {
    if let Some(window) = app.get_window("main") {
        let was_minimized = window.is_minimized().unwrap_or(false);
        let _ = window.show();
        if was_minimized {
            let _ = window.unminimize();
        }
        let _ = window.set_focus();

        #[cfg(target_os = "windows")]
        unsafe {
            if let Ok(hwnd_raw) = window.hwnd() {
                let hwnd = hwnd_raw.0 as isize;
                if hwnd != 0 {
                    if win32::IsIconic(hwnd) != 0 {
                        win32::ShowWindow(hwnd, win32::SW_RESTORE);
                    } else {
                        win32::ShowWindow(hwnd, win32::SW_SHOW);
                    }
                    win32::BringWindowToTop(hwnd);
                    win32::SetForegroundWindow(hwnd);
                }
            }
            ensure_sidebar_on_top(&window);
        }
        sync_window_layout(app);
    }
}

pub fn sync_window_layout(app: &AppHandle) {
    let window = match app.get_window("main") {
        Some(w) => w,
        None => return,
    };
    let logical_size = match get_window_logical_size(&window) {
        Some(s) => s,
        None => return,
    };

    // Cancelar animaciones en curso para que no sobrescriban el tamaño tras un resize/restore.
    SWITCH_ANIM_ID.fetch_add(1, Ordering::SeqCst);
    SIDEBAR_ANIM_ID.fetch_add(1, Ordering::SeqCst);
    SIDEBAR_ANIM_ACTIVE.store(false, Ordering::SeqCst);

    let target_tenths = TARGET_WIDTH_TENTHS.load(Ordering::SeqCst).clamp(800, 2800);
    CURRENT_WIDTH_TENTHS.store(target_tenths, Ordering::SeqCst);
    TARGET_WIDTH_TENTHS.store(target_tenths, Ordering::SeqCst);

    let active_session_id = app.try_state::<AppState>().and_then(|st| {
        st.active_session_id
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    });
    let is_settings = active_session_id.is_none();
    let intro_active = INTRO_ACTIVE.load(Ordering::SeqCst);

    if let Some(main_wv) = app.get_webview("main") {
        let _ = main_wv.set_auto_resize(false);
        let _ = main_wv.set_position(LogicalPosition::new(0.0, 0.0));
        if is_settings || intro_active {
            let _ = main_wv.set_size(LogicalSize::new(logical_size.width, logical_size.height));
        } else {
            let final_w = clamp_sidebar_width(target_tenths as f64 / 10.0);
            let _ = main_wv.set_size(LogicalSize::new(final_w, logical_size.height));
        }
    }

    let content_width = (logical_size.width - 80.0).max(100.0);
    let height = logical_size.height.max(100.0);
    let active_label = active_session_id
        .as_ref()
        .map(|id| format!("session_{}", id));

    for (label, wv) in app.webviews() {
        if label == "main" {
            continue;
        }
        let _ = wv.set_auto_resize(false);
        if intro_active {
            let _ = wv.set_position(LogicalPosition::new(-9000.0, -9000.0));
            let _ = wv.set_size(LogicalSize::new(content_width, height));
        } else if !is_settings && Some(&label) == active_label.as_ref() {
            let _ = wv.set_position(LogicalPosition::new(80.0, 0.0));
            let _ = wv.set_size(LogicalSize::new(content_width, height));
            let _ = wv.show();
        } else {
            let _ = wv.set_position(LogicalPosition::new(-9000.0, -9000.0));
            let _ = wv.set_size(LogicalSize::new(content_width, height));
            let _ = wv.hide();
        }
    }

    #[cfg(target_os = "windows")]
    unsafe {
        ensure_sidebar_on_top(&window);
    }
}

mod commands {
    use super::*;

    #[tauri::command]
    pub fn get_settings(app: AppHandle, state: tauri::State<'_, AppState>) -> AppSettings {
        let mut settings = state
            .settings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Ok(enabled) = app.autolaunch().is_enabled() {
            settings.open_at_login = enabled;
        }
        settings
    }

    #[tauri::command]
    pub fn set_close_to_tray(
        app: AppHandle,
        state: tauri::State<'_, AppState>,
        enable: bool,
    ) -> Result<(), String> {
        let mut settings = state.settings.lock().unwrap_or_else(|e| e.into_inner());
        settings.close_to_tray = enable;
        persist_settings(&app, &settings)
    }

    #[tauri::command]
    pub fn set_start_minimized(
        app: AppHandle,
        state: tauri::State<'_, AppState>,
        enable: bool,
    ) -> Result<(), String> {
        let mut settings = state.settings.lock().unwrap_or_else(|e| e.into_inner());
        settings.start_minimized = enable;
        persist_settings(&app, &settings)
    }

    #[tauri::command]
    pub fn minimize_to_tray(window: tauri::WebviewWindow) -> Result<(), String> {
        window
            .hide()
            .map_err(|e| format!("Error ocultando ventana: {}", e))
    }

    #[tauri::command]
    pub fn quit_app(app: AppHandle) {
        app.exit(0);
    }

    #[tauri::command]
    pub async fn open_external_url(url: String) -> Result<(), String> {
        external_url::validate_external_url(&url)?;
        tauri::async_runtime::spawn_blocking(move || external_url::open_external_url_blocking(&url))
            .await
            .map_err(|e| format!("Error ejecutando tarea de enlace: {}", e))?
    }

    #[tauri::command]
    pub async fn copy_image_to_clipboard(
        width: u32,
        height: u32,
        rgba_base64: String,
        png_base64: Option<String>,
    ) -> Result<(), String> {
        tauri::async_runtime::spawn_blocking(move || {
            clipboard::copy_image_blocking(width, height, &rgba_base64, png_base64.as_deref())
        })
        .await
        .map_err(|e| format!("Error copiando imagen: {}", e))?
    }

    #[tauri::command]
    pub async fn copy_video_to_clipboard(
        data_base64: String,
        extension: String,
    ) -> Result<(), String> {
        tauri::async_runtime::spawn_blocking(move || {
            clipboard::copy_video_blocking(&data_base64, &extension)
        })
        .await
        .map_err(|e| format!("Error copiando video: {}", e))?
    }

    #[tauri::command]
    pub fn reload_session(app: AppHandle, session_id: String) -> Result<(), String> {
        let clean_id = sanitize_session_id(&session_id)?;
        let label = format!("session_{}", clean_id);
        let wv = app
            .get_webview(&label)
            .ok_or_else(|| format!("Sesión {} no encontrada", clean_id))?;
        wv.reload()
            .map_err(|e| format!("Error recargando sesión: {}", e))
    }

    #[tauri::command]
    pub fn fade_session_webview(
        app: AppHandle,
        session_id: String,
        opacity: f64,
        duration_ms: Option<u64>,
    ) -> Result<(), String> {
        let clean_id = sanitize_session_id(&session_id)?;
        let label = format!("session_{}", clean_id);
        let Some(wv) = app.get_webview(&label) else {
            return Ok(());
        };
        let clamped_op = if opacity.is_finite() {
            opacity.clamp(0.0, 1.0)
        } else {
            1.0
        };
        let ms = duration_ms.unwrap_or(140).min(1000);
        let script = format!(
            r#"
            (function() {{
                const root = document.documentElement;
                if (!root) return;
                root.style.backgroundColor = '#12150e';
                root.style.willChange = 'opacity';
                root.style.transition = 'opacity {ms}ms cubic-bezier(0.16, 1, 0.3, 1)';
                root.style.opacity = '{clamped_op:.3}';
            }})();
            "#
        );
        wv.eval(&script)
            .map_err(|e| format!("Error animando opacidad de sesión: {}", e))
    }

    #[tauri::command]
    pub fn prepare_session_under_settings(app: AppHandle, session_id: String) -> Result<(), String> {
        let clean_id = sanitize_session_id(&session_id)?;
        let label = format!("session_{}", clean_id);
        let window = app
            .get_window("main")
            .ok_or("Ventana principal no encontrada")?;
        let logical_size = match get_window_logical_size(&window) {
            Some(s) => s,
            None => return Ok(()),
        };
        let height = logical_size.height.max(100.0);
        let content_width = (logical_size.width - 80.0).max(100.0);

        if let Some(main_wv) = app.get_webview("main") {
            let _ = main_wv.set_auto_resize(false);
            let _ = main_wv.set_position(LogicalPosition::new(0.0, 0.0));
            let _ = main_wv.set_size(LogicalSize::new(logical_size.width, logical_size.height));
            let _ = main_wv.show();
        }

        if let Some(wv) = app.get_webview(&label) {
            let _ = wv.set_size(LogicalSize::new(content_width, height));
            let _ = wv.set_position(LogicalPosition::new(80.0, 0.0));
            let _ = wv.eval(
                "if (document.documentElement) { document.documentElement.style.transition = 'none'; document.documentElement.style.opacity = '1'; }",
            );
            let _ = wv.show();
        }

        #[cfg(target_os = "windows")]
        unsafe {
            ensure_sidebar_on_top(&window);
        }
        Ok(())
    }

    #[tauri::command]
    pub async fn check_for_updates() -> Result<updater::UpdateInfo, String> {
        tauri::async_runtime::spawn_blocking(updater::check_for_updates_blocking)
            .await
            .map_err(|e| format!("Error buscando actualizaciones: {}", e))?
    }

    #[tauri::command]
    pub async fn install_update(
        app: AppHandle,
        download_url: String,
        asset_name: String,
        is_portable: bool,
    ) -> Result<(), String> {
        tauri::async_runtime::spawn_blocking(move || {
            updater::download_and_install_update_blocking(
                &app,
                &download_url,
                &asset_name,
                is_portable,
            )
        })
        .await
        .map_err(|e| format!("Error instalando actualización: {}", e))?
    }

    #[tauri::command]
    pub async fn create_session_webview(
        app: AppHandle,
        session_id: String,
        sidebar_width: Option<f64>,
        is_active: Option<bool>,
    ) -> Result<(), String> {
        let clean_id = sanitize_session_id(&session_id)?.to_string();
        let _width_offset = clamp_sidebar_width(sidebar_width.unwrap_or(80.0));
        let active = is_active.unwrap_or(false);
        let window = app
            .get_window("main")
            .ok_or("Ventana principal no encontrada")?;
        let label = format!("session_{}", clean_id);
        if app.get_webview(&label).is_some() {
            return Ok(());
        }

        let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let data_dir = app_dir.join("sessions").join(&clean_id);
        let url: tauri::Url = "https://web.whatsapp.com"
            .parse()
            .map_err(|e| format!("{:?}", e))?;

        let init_script = session_script::build_init_script(&clean_id);

        let builder = WebviewBuilder::new(&label, WebviewUrl::External(url))
            .data_directory(data_dir)
            .background_color(tauri::window::Color(18, 21, 14, 255))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .initialization_script(&init_script)
            .on_new_window(|url, _features| {
                if matches!(url.scheme(), "http" | "https" | "mailto") {
                    external_url::open_external_url_detached(url.as_str().to_string());
                }
                NewWindowResponse::Deny
            })
            .on_navigation(|url| {
                if external_url::should_allow_webview_navigation(url) {
                    return true;
                }
                if matches!(url.scheme(), "http" | "https" | "mailto") {
                    external_url::open_external_url_detached(url.as_str().to_string());
                }
                false
            });

        let logical_size =
            get_window_logical_size(&window).unwrap_or_else(|| LogicalSize::new(1400.0, 900.0));
        let intro_active = INTRO_ACTIVE.load(Ordering::SeqCst);

        // Durante el intro (o si no es activa), crear siempre fuera de pantalla en (-9000, -9000)
        // para que Win32 al crear el HWND hijo en HWND_TOP jamás provoque un parpadeo sobre el Intro Screen.
        let pos = if active && !intro_active {
            LogicalPosition::new(80.0, 0.0)
        } else {
            LogicalPosition::new(-9000.0, -9000.0)
        };
        let content_width = (logical_size.width - 80.0).max(100.0);
        let height = logical_size.height.max(100.0);
        let size = LogicalSize::new(content_width, height);

        let wv = window
            .add_child(builder, pos, size)
            .map_err(|e| e.to_string())?;
        let _ = wv.set_auto_resize(false);
        if active {
            if let Some(st) = app.try_state::<AppState>() {
                let mut guard = st
                    .active_session_id
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                *guard = Some(clean_id);
            }
            let _ = wv.show();
        } else if intro_active {
            let _ = wv.show();
        } else {
            let _ = wv.hide();
        }

        if !intro_active {
            #[cfg(target_os = "windows")]
            unsafe {
                ensure_sidebar_on_top(&window);
            }
        }
        Ok(())
    }

    #[tauri::command]
    pub fn switch_session(
        app: AppHandle,
        active_session_id: Option<String>,
        previous_session_id: Option<String>,
        all_session_ids: Vec<String>,
        sidebar_width: Option<f64>,
        direction: Option<i32>,
    ) -> Result<(), String> {
        let _ = &all_session_ids;
        if let Some(st) = app.try_state::<AppState>() {
            let mut guard = st
                .active_session_id
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            *guard = active_session_id.clone();
        }

        let is_settings = active_session_id.is_none();
        let fallback_w = clamp_sidebar_width(CURRENT_WIDTH_TENTHS.load(Ordering::SeqCst) as f64 / 10.0);
        let final_w = match sidebar_width {
            Some(w) => clamp_sidebar_width(w),
            None => fallback_w,
        };

        // Cancelar cualquier animación en curso del sidebar para evitar condiciones de carrera
        SIDEBAR_ANIM_ID.fetch_add(1, Ordering::SeqCst);
        SIDEBAR_ANIM_ACTIVE.store(false, Ordering::SeqCst);
        if !is_settings {
            let tenths = (final_w * 10.0) as u32;
            CURRENT_WIDTH_TENTHS.store(tenths, Ordering::SeqCst);
            TARGET_WIDTH_TENTHS.store(tenths, Ordering::SeqCst);
        }

        let window = app
            .get_window("main")
            .ok_or("Ventana principal no encontrada")?;
        let logical_size = match get_window_logical_size(&window) {
            Some(s) => s,
            None => return Ok(()),
        };

        // Si el intro aún está activo, mantener main_wv cubriendo toda la ventana
        if INTRO_ACTIVE.load(Ordering::SeqCst) {
            if let Some(main_wv) = app.get_webview("main") {
                let _ = main_wv.set_auto_resize(false);
                let _ = main_wv.set_position(LogicalPosition::new(0.0, 0.0));
                let _ = main_wv.set_size(LogicalSize::new(logical_size.width, logical_size.height));
                let _ = main_wv.show();
            }
            return Ok(());
        }

        let height = logical_size.height.max(100.0);
        let content_width = (logical_size.width - 80.0).max(100.0);

        // Si el destino es Ajustes, expandir main_wv de inmediato y ocultar todas las sesiones sin animar detrás.
        if is_settings {
            SWITCH_ANIM_ID.fetch_add(1, Ordering::SeqCst);
            if let Some(main_wv) = app.get_webview("main") {
                let _ = main_wv.set_auto_resize(false);
                let _ = main_wv.set_position(LogicalPosition::new(0.0, 0.0));
                let _ = main_wv.set_size(LogicalSize::new(logical_size.width, logical_size.height));
                let _ = main_wv.show();
                let _ = main_wv.set_focus();
            }
            for (label, wv) in app.webviews() {
                if label == "main" {
                    continue;
                }
                let _ = wv.set_position(LogicalPosition::new(-9000.0, -9000.0));
                let _ = wv.hide();
            }
            #[cfg(target_os = "windows")]
            unsafe {
                ensure_sidebar_on_top(&window);
            }
            return Ok(());
        }

        let dir = direction.unwrap_or(0);
        let incoming_label = active_session_id
            .as_ref()
            .map(|id| format!("session_{}", id));
        let outgoing_label = previous_session_id
            .as_ref()
            .map(|id| format!("session_{}", id));

        let incoming_wv = incoming_label.as_ref().and_then(|lbl| app.get_webview(lbl));
        let outgoing_wv = outgoing_label.as_ref().and_then(|lbl| app.get_webview(lbl));

        // Sin animación (fin del intro, salida de Ajustes sin sesión previa visible, misma sesión o dir == 0):
        // posicionar primero la sesión entrante en (80, 0) ANTES de reducir main_wv para evitar frames vacíos.
        if dir == 0 || incoming_label == outgoing_label || outgoing_wv.is_none() {
            SWITCH_ANIM_ID.fetch_add(1, Ordering::SeqCst);

            if let Some(ref inc) = incoming_wv {
                let _ = inc.set_size(LogicalSize::new(content_width, height));
                let _ = inc.set_position(LogicalPosition::new(80.0, 0.0));
                let _ = inc.eval(
                    "if (document.documentElement) { document.documentElement.style.opacity = '1'; }",
                );
                let _ = inc.show();
            }

            for (label, wv) in app.webviews() {
                if label == "main" || Some(&label) == incoming_label.as_ref() {
                    continue;
                }
                let _ = wv.set_position(LogicalPosition::new(-9000.0, -9000.0));
                let _ = wv.hide();
            }

            #[cfg(target_os = "windows")]
            unsafe {
                ensure_sidebar_on_top(&window);
            }

            if let Some(main_wv) = app.get_webview("main") {
                let _ = main_wv.set_auto_resize(false);
                let _ = main_wv.set_position(LogicalPosition::new(0.0, 0.0));
                let _ = main_wv.set_size(LogicalSize::new(final_w, logical_size.height));
                let _ = main_wv.show();
            }
            return Ok(());
        }

        // Configurar webview principal cuando hay animación direccional entre dos sesiones
        if let Some(main_wv) = app.get_webview("main") {
            let _ = main_wv.set_auto_resize(false);
            let _ = main_wv.set_position(LogicalPosition::new(0.0, 0.0));
            let _ = main_wv.set_size(LogicalSize::new(final_w, logical_size.height));
            let _ = main_wv.show();
        }

        // Ocultar cualquier otra sesión (incluidas huérfanas) que no sea incoming ni outgoing
        for (label, wv) in app.webviews() {
            if label == "main" {
                continue;
            }
            if Some(&label) != incoming_label.as_ref() && Some(&label) != outgoing_label.as_ref() {
                let _ = wv.set_position(LogicalPosition::new(-9000.0, -9000.0));
                let _ = wv.hide();
            }
        }

        let (inc_start_y, inc_end_y) = if dir > 0 { (height, 0.0) } else { (-height, 0.0) };
        let (out_start_y, out_end_y) = if dir > 0 { (0.0, -height) } else { (0.0, height) };

        if let Some(ref inc) = incoming_wv {
            let _ = inc.set_size(LogicalSize::new(content_width, height));
            let _ = inc.set_position(LogicalPosition::new(80.0, inc_start_y));
            let _ = inc.eval(
                "if (document.documentElement) { document.documentElement.style.opacity = '1'; }",
            );
            let _ = inc.show();
        }
        if let Some(ref out) = outgoing_wv {
            let _ = out.set_size(LogicalSize::new(content_width, height));
            let _ = out.set_position(LogicalPosition::new(80.0, out_start_y));
            let _ = out.show();
        }

        #[cfg(target_os = "windows")]
        unsafe {
            ensure_sidebar_on_top(&window);
        }

        let anim_id = SWITCH_ANIM_ID.fetch_add(1, Ordering::SeqCst) + 1;
        let inc_clone = incoming_wv.clone();
        let out_clone = outgoing_wv.clone();
        let window_clone = window.clone();

        std::thread::spawn(move || {
            frame::begin();
            let (x1, y1, x2, y2) = SLIDE_CURVE;
            let start = std::time::Instant::now();

            loop {
                if SWITCH_ANIM_ID.load(Ordering::SeqCst) != anim_id {
                    break;
                }
                let step_start = std::time::Instant::now();
                let p = (start.elapsed().as_secs_f64() / SLIDE_SECS).min(1.0);
                let ease = cubic_bezier(x1, y1, x2, y2, p);
                let inc_y = (inc_start_y + (inc_end_y - inc_start_y) * ease).round();
                let out_y = (out_start_y + (out_end_y - out_start_y) * ease).round();

                let inc_step = inc_clone.clone();
                let out_step = out_clone.clone();
                run_frame_sync(&window_clone, move || {
                    if let Some(ref inc) = inc_step {
                        let _ = inc.set_position(LogicalPosition::new(80.0, inc_y));
                    }
                    if let Some(ref out) = out_step {
                        let _ = out.set_position(LogicalPosition::new(80.0, out_y));
                    }
                });

                if p >= 1.0 {
                    break;
                }
                frame::wait_remaining(step_start.elapsed());
            }
            frame::end();

            if SWITCH_ANIM_ID.load(Ordering::SeqCst) == anim_id {
                let inc_final = inc_clone.clone();
                let out_final = out_clone.clone();
                let win_final = window_clone.clone();
                run_frame_sync(&window_clone, move || {
                    if let Some(ref inc) = inc_final {
                        let _ = inc.set_position(LogicalPosition::new(80.0, 0.0));
                    }
                    if let Some(ref out) = out_final {
                        let _ = out.set_position(LogicalPosition::new(-9000.0, -9000.0));
                        let _ = out.hide();
                    }
                    #[cfg(target_os = "windows")]
                    unsafe {
                        ensure_sidebar_on_top(&win_final);
                    }
                });
            }
        });

        Ok(())
    }

    #[tauri::command]
    pub fn update_sidebar_bounds(
        app: AppHandle,
        active_session_id: Option<String>,
        sidebar_width: f64,
    ) -> Result<(), String> {
        if let Some(st) = app.try_state::<AppState>() {
            let mut guard = st
                .active_session_id
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            *guard = active_session_id.clone();
        }
        let is_settings = active_session_id.is_none();
        let intro_active = INTRO_ACTIVE.load(Ordering::SeqCst);
        let window = app
            .get_window("main")
            .ok_or("Ventana principal no encontrada")?;
        let target_w = clamp_sidebar_width(sidebar_width);
        let target_tenths = (target_w * 10.0) as u32;

        let logical_size = match get_window_logical_size(&window) {
            Some(s) => s,
            None => {
                if !is_settings && !intro_active {
                    CURRENT_WIDTH_TENTHS.store(target_tenths, Ordering::SeqCst);
                    TARGET_WIDTH_TENTHS.store(target_tenths, Ordering::SeqCst);
                }
                return Ok(());
            }
        };

        if is_settings || intro_active {
            SIDEBAR_ANIM_ID.fetch_add(1, Ordering::SeqCst);
            SIDEBAR_ANIM_ACTIVE.store(false, Ordering::SeqCst);
            if let Some(main_wv) = app.get_webview("main") {
                let _ = main_wv.set_auto_resize(false);
                let _ = main_wv.set_size(LogicalSize::new(logical_size.width, logical_size.height));
            }
            return Ok(());
        }

        if let Some(main_wv) = app.get_webview("main") {
            let _ = main_wv.set_auto_resize(false);
            let prev_target = TARGET_WIDTH_TENTHS.swap(target_tenths, Ordering::SeqCst);
            let curr_tenths = CURRENT_WIDTH_TENTHS.load(Ordering::SeqCst);
            let curr_w = curr_tenths as f64 / 10.0;

            if prev_target == target_tenths && SIDEBAR_ANIM_ACTIVE.load(Ordering::SeqCst) {
                return Ok(());
            }

            if (curr_w - target_w).abs() < 1.0 {
                SIDEBAR_ANIM_ID.fetch_add(1, Ordering::SeqCst);
                SIDEBAR_ANIM_ACTIVE.store(false, Ordering::SeqCst);
                CURRENT_WIDTH_TENTHS.store(target_tenths, Ordering::SeqCst);
                let _ = main_wv.set_size(LogicalSize::new(target_w, logical_size.height));
                return Ok(());
            }

            let anim_id = SIDEBAR_ANIM_ID.fetch_add(1, Ordering::SeqCst) + 1;
            SIDEBAR_ANIM_ACTIVE.store(true, Ordering::SeqCst);
            let height = logical_size.height;
            let main_wv_clone = main_wv.clone();
            let window_clone = window.clone();
            let is_collapsing = target_w < curr_w;

            std::thread::spawn(move || {
                frame::begin();
                let (dur, (x1, y1, x2, y2)) = if is_collapsing {
                    (COLLAPSE_SECS, COLLAPSE_CURVE)
                } else {
                    (EXPAND_SECS, EXPAND_CURVE)
                };
                let start = std::time::Instant::now();
                let mut last_w = curr_w.round();

                loop {
                    if SIDEBAR_ANIM_ID.load(Ordering::SeqCst) != anim_id {
                        break;
                    }
                    let step_start = std::time::Instant::now();
                    let p = (start.elapsed().as_secs_f64() / dur).min(1.0);
                    let ease = cubic_bezier(x1, y1, x2, y2, p);
                    let w = (curr_w + (target_w - curr_w) * ease).round();
                    CURRENT_WIDTH_TENTHS.store((w * 10.0) as u32, Ordering::SeqCst);

                    if (w - last_w).abs() >= 1.0 || p >= 1.0 {
                        last_w = w;
                        let wv_step = main_wv_clone.clone();
                        run_frame_sync(&window_clone, move || {
                            let _ = wv_step.set_size(LogicalSize::new(w, height));
                        });
                    }

                    if p >= 1.0 {
                        break;
                    }
                    frame::wait_remaining(step_start.elapsed());
                }
                frame::end();

                if SIDEBAR_ANIM_ID.load(Ordering::SeqCst) == anim_id {
                    CURRENT_WIDTH_TENTHS.store(target_tenths, Ordering::SeqCst);
                    SIDEBAR_ANIM_ACTIVE.store(false, Ordering::SeqCst);
                    let wv_final = main_wv_clone.clone();
                    run_frame_sync(&window_clone, move || {
                        let _ = wv_final.set_size(LogicalSize::new(target_w, height));
                    });
                }
            });
        }

        Ok(())
    }

    #[tauri::command]
    pub fn finish_intro(
        app: AppHandle,
        active_session_id: Option<String>,
        all_session_ids: Vec<String>,
        sidebar_width: Option<f64>,
    ) -> Result<(), String> {
        INTRO_ACTIVE.store(false, Ordering::SeqCst);
        switch_session(
            app,
            active_session_id,
            None,
            all_session_ids,
            sidebar_width,
            Some(0),
        )
    }

    #[tauri::command]
    pub fn destroy_session(app: AppHandle, session_id: String) -> Result<(), String> {
        let clean_id = sanitize_session_id(&session_id)?;
        let label = format!("session_{}", clean_id);

        let mut was_active = false;
        if let Some(st) = app.try_state::<AppState>() {
            let mut guard = st
                .active_session_id
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if guard.as_deref() == Some(clean_id) {
                *guard = None;
                was_active = true;
            }
        }
        if was_active {
            SWITCH_ANIM_ID.fetch_add(1, Ordering::SeqCst);
        }

        if let Some(wv) = app.get_webview(&label) {
            let _ = wv.close();
        }

        let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let data_dir = app_dir.join("sessions").join(clean_id);
        if data_dir.exists() {
            // WebView2 libera los archivos de sesión de forma asíncrona tras wv.close()
            std::thread::spawn(move || {
                for attempt in 0..16 {
                    if !data_dir.exists() {
                        break;
                    }
                    if fs::remove_dir_all(&data_dir).is_ok() {
                        break;
                    }
                    if attempt < 15 {
                        std::thread::sleep(std::time::Duration::from_millis(150));
                    }
                }
            });
        }
        Ok(())
    }

    #[tauri::command]
    pub fn report_unread(app: AppHandle, session_id: String, unread: u32) {
        #[derive(Serialize, Clone)]
        struct UnreadPayload {
            session_id: String,
            unread: u32,
        }
        let _ = app.emit("session-unread-update", UnreadPayload { session_id, unread });
    }

    #[tauri::command]
    pub fn report_session_ready(app: AppHandle, session_id: String) {
        #[derive(Serialize, Clone)]
        struct ReadyPayload {
            session_id: String,
        }
        let _ = app.emit("session-ready", ReadyPayload { session_id });
    }

    #[tauri::command]
    pub fn report_session_avatar(app: AppHandle, session_id: String, avatar_data_url: String) {
        let Ok(clean_id) = sanitize_session_id(&session_id) else {
            return;
        };
        if !is_allowed_avatar_payload(&avatar_data_url) {
            return;
        }
        #[derive(Serialize, Clone)]
        struct AvatarPayload {
            session_id: String,
            avatar: String,
        }
        let _ = app.emit(
            "session-avatar-update",
            AvatarPayload {
                session_id: clean_id.to_string(),
                avatar: avatar_data_url.trim().to_string(),
            },
        );
    }

    #[tauri::command]
    pub fn report_webview_click(app: AppHandle) {
        let _ = app.emit("webview-clicked", ());
    }

    #[tauri::command]
    pub fn show_toast(app: AppHandle, message: String, toast_type: String) {
        let active_session = app.try_state::<AppState>().and_then(|st| {
            st.active_session_id
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
        });

        let Some(active_id) = active_session else {
            return;
        };

        let target_label = format!("session_{}", active_id);
        let Some(wv) = app.get_webview(&target_label) else {
            return;
        };

        let msg_json = serde_json::to_string(&message).unwrap_or_else(|_| "\"\"".to_string());
        let type_json =
            serde_json::to_string(&toast_type).unwrap_or_else(|_| "\"success\"".to_string());

        let script = format!(
            r#"
            (function() {{
                const msg = {msg_json};
                const tType = {type_json};
                const tryShow = () => {{
                    if (typeof window.__suppShowToast === 'function') {{
                        window.__suppShowToast(msg, tType);
                        return true;
                    }}
                    return false;
                }};
                if (!tryShow()) {{
                    let attempts = 0;
                    const interval = setInterval(() => {{
                        attempts++;
                        if (tryShow() || attempts > 15) {{
                            clearInterval(interval);
                        }}
                    }}, 200);
                }}
            }})();
            "#
        );
        let _ = wv.eval(&script);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_and_focus_main_window(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(|app| {
            let initial_settings = load_settings(app.handle());
            let launched_minimized = std::env::args().any(|arg| arg == "--minimized");
            let start_minimized_flag = initial_settings.start_minimized || launched_minimized;

            app.manage(AppState {
                settings: Mutex::new(initial_settings),
                active_session_id: Mutex::new(None),
            });

            if let Some(main_wv) = app.get_webview("main") {
                let _ = main_wv.set_auto_resize(false);
            }

            // Configurar menú nativo de la bandeja del sistema (System Tray)
            let quit_i = MenuItem::with_id(app, "quit", "Salir de Supp", true, None::<&str>)?;
            let show_i = MenuItem::with_id(app, "show", "Abrir Supp", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

            let mut tray_builder = TrayIconBuilder::new()
                .tooltip("Supp - WhatsApp Multi-Sesión")
                .menu(&menu)
                .show_menu_on_left_click(false);

            if let Some(icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(icon.clone());
            }

            let _tray = tray_builder
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    "show" => {
                        show_and_focus_main_window(app);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| match event {
                    TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    } => {
                        show_and_focus_main_window(tray.app_handle());
                    }
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } => {
                        show_and_focus_main_window(tray.app_handle());
                    }
                    _ => {}
                })
                .build(app)?;

            // Configurar comportamiento de la ventana principal
            if let Some(window) = app.get_window("main") {
                if start_minimized_flag {
                    let _ = window.hide();
                } else {
                    let _ = window.show();
                    let _ = window.set_focus();
                    sync_window_layout(app.handle());
                }

                let window_clone = window.clone();
                let app_handle = app.handle().clone();
                window.on_window_event(move |event| match event {
                    WindowEvent::CloseRequested { api, .. } => {
                        let state = app_handle.state::<AppState>();
                        let should_close_to_tray = state
                            .settings
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .close_to_tray;
                        if should_close_to_tray {
                            api.prevent_close();
                            let _ = window_clone.hide();
                        }
                    }
                    WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                        sync_window_layout(&app_handle);
                    }
                    _ => {}
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_close_to_tray,
            commands::set_start_minimized,
            commands::minimize_to_tray,
            commands::quit_app,
            commands::create_session_webview,
            commands::switch_session,
            commands::update_sidebar_bounds,
            commands::finish_intro,
            commands::destroy_session,
            commands::reload_session,
            commands::fade_session_webview,
            commands::prepare_session_under_settings,
            commands::check_for_updates,
            commands::install_update,
            commands::open_external_url,
            commands::copy_image_to_clipboard,
            commands::copy_video_to_clipboard,
            commands::report_unread,
            commands::report_session_ready,
            commands::report_session_avatar,
            commands::report_webview_click,
            commands::show_toast,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_session_ids() {
        assert_eq!(sanitize_session_id("abc-123_DEF").unwrap(), "abc-123_DEF");
        assert!(sanitize_session_id("").is_err());
        assert!(sanitize_session_id("../evil").is_err());
        assert!(sanitize_session_id("a/b").is_err());
        assert!(sanitize_session_id("a\\b").is_err());
        assert!(sanitize_session_id("bad id").is_err());
    }

    #[test]
    fn clamps_sidebar_width_bounds() {
        assert_eq!(clamp_sidebar_width(40.0), 80.0);
        assert_eq!(clamp_sidebar_width(180.0), 180.0);
        assert_eq!(clamp_sidebar_width(500.0), 280.0);
        assert_eq!(clamp_sidebar_width(f64::NAN), 80.0);
    }

    #[test]
    fn validates_avatar_payloads() {
        assert!(is_allowed_avatar_payload("data:image/jpeg;base64,/9j/4AAQ"));
        assert!(is_allowed_avatar_payload("https://pps.whatsapp.net/v/t61.24694-24/abc.jpg?oe=123"));
        assert!(is_allowed_avatar_payload("https://scontent.fbcdn.net/v/t39/avatar.jpg"));
        assert!(is_allowed_avatar_payload("https://web.whatsapp.com/avatar.png"));

        assert!(!is_allowed_avatar_payload(""));
        assert!(!is_allowed_avatar_payload("javascript:alert(1)"));
        assert!(!is_allowed_avatar_payload("http://pps.whatsapp.net/avatar.jpg"));
        assert!(!is_allowed_avatar_payload("https://static.whatsapp.net/rsrc.php/meta-ai.png"));
        assert!(!is_allowed_avatar_payload("https://evil.example.com/avatar.jpg"));
        assert!(!is_allowed_avatar_payload("https://whatsapp.net.evil.com/a.jpg"));
    }
}
