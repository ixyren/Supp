//! Floating square Supp desktop notification popups on Windows.
//!
//! Renders customizable square notification cards styled with Supp's active
//! [`Palette`], displaying the Supp header icon and name, contact avatar,
//! notification details, live countdown progress bar (paused on hover), and
//! selectable entry animations anchored to any screen corner.
//!
//! Popups live on a dedicated Win32 UI thread with a hidden message-only
//! controller window so notifications work both while the main window is open
//! and while Supp sits in the system tray with no eframe window running.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Dwm::{
    DWMWA_BORDER_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_ROUND, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, BitBlt, CLEARTYPE_QUALITY,
    CLIP_DEFAULT_PRECIS, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreatePen,
    CreateRoundRectRgn, CreateSolidBrush, DEFAULT_CHARSET, DIB_RGB_COLORS, DT_CENTER,
    DT_END_ELLIPSIS, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, DeleteDC,
    DeleteObject, DrawTextW, EndPaint, FF_DONTCARE, FW_BOLD, FW_NORMAL, FW_SEMIBOLD, FillRect,
    GetStockObject, HFONT, HGDIOBJ, InvalidateRect, NULL_BRUSH, NULL_PEN, OUT_DEFAULT_PRECIS,
    PAINTSTRUCT, PS_SOLID, RoundRect, SRCCOPY, ScreenToClient, SelectObject, SetBkMode,
    SetDIBitsToDevice, SetTextColor, SetWindowRgn, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetThreadDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GWLP_USERDATA, GetClientRect, GetCursorPos, GetMessageW, GetWindowLongPtrW, HWND_MESSAGE,
    HWND_TOPMOST, IDC_ARROW, IDC_HAND, KillTimer, LWA_ALPHA, LoadCursorW, MA_NOACTIVATE, MSG,
    PostMessageW, RegisterClassExW, SPI_GETWORKAREA, SW_SHOWNOACTIVATE, SWP_NOACTIVATE,
    SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    SetCursor, SetLayeredWindowAttributes, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    SystemParametersInfoW, TranslateMessage, WM_APP, WM_DESTROY, WM_ERASEBKGND, WM_LBUTTONUP,
    WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_NCDESTROY, WM_PAINT, WM_RBUTTONUP, WM_TIMER, WNDCLASSEXW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{BOOL, PCWSTR};

use crate::notify::NotificationTarget;
use crate::settings::{NotificationCorner, NotificationEffect, NotificationSize, Settings};
use crate::theme::Palette;

const MARGIN_EDGE: f32 = 16.0;
const STACK_GAP: f32 = 10.0;
const CORNER_RADIUS: f32 = 16.0;
const HEADER_HEIGHT: f32 = 36.0;
const APP_ICON_SIZE: f32 = 18.0;
const CLOSE_SIZE: f32 = 22.0;
const CLOSE_RIGHT_INSET: f32 = 10.0;
const PROGRESS_HEIGHT: f32 = 5.0;
const PROGRESS_BOTTOM_INSET: f32 = 10.0;
const PROGRESS_SIDE_INSET: f32 = 14.0;
const MAX_VISIBLE: usize = 3;

const LEAVE_DURATION: Duration = Duration::from_millis(160);
const RESUME_MIN_REMAINING: Duration = Duration::from_millis(1500);
const SHIMMER_DURATION_MS: u64 = 640;

const TIMER_TICK: usize = 1;
const TICK_MS: u32 = 16;
const WM_APP_CMD: u32 = WM_APP + 1;
const WM_MOUSELEAVE: u32 = 0x02A3;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Returns a unique popup identifier for cancellation and tracking.
pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Appearance and behavior configuration for Windows desktop popup notifications.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupConfig {
    pub palette: Palette,
    pub corner: NotificationCorner,
    pub size: NotificationSize,
    pub duration_secs: u32,
    pub show_progress_bar: bool,
    pub opacity: u8,
    pub effect: NotificationEffect,
}

impl Default for PopupConfig {
    fn default() -> Self {
        Self::from_settings(Palette::dark(), &Settings::default())
    }
}

impl PopupConfig {
    /// Builds a [`PopupConfig`] from Supp's active palette and user settings.
    pub fn from_settings(palette: Palette, settings: &Settings) -> Self {
        Self {
            palette,
            corner: settings.notification_position,
            size: settings.notification_size,
            duration_secs: settings.notification_duration_secs.clamp(2, 30),
            show_progress_bar: settings.notification_progress_bar,
            opacity: settings.notification_opacity.clamp(70, 100),
            effect: settings.notification_effect,
        }
    }

    fn max_alpha(self) -> u8 {
        ((f32::from(self.opacity.clamp(70, 100)) / 100.0) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8
    }
}

/// Request to show a floating popup notification on Windows.
pub struct PopupRequest {
    pub id: u64,
    pub title: String,
    pub body: String,
    pub picture: Option<PathBuf>,
    pub config: PopupConfig,
    pub target: NotificationTarget,
    pub opened: Arc<Mutex<Vec<NotificationTarget>>>,
    pub wake: Arc<dyn Fn() + Send + Sync>,
    pub is_call: bool,
}

enum Command {
    Show(PopupRequest),
    Close(u64),
    CloseAll,
}

struct Manager {
    queue: Arc<Mutex<VecDeque<Command>>>,
    ctrl_hwnd: isize,
}

static MANAGER: OnceLock<Option<Manager>> = OnceLock::new();

/// Shows a square Supp popup card anchored to the configured corner.
pub fn show(req: PopupRequest) {
    send_command(Command::Show(req));
}

/// Dismisses a visible popup card by its registration id.
pub fn close(id: u64) {
    send_command(Command::Close(id));
}

/// Dismisses every visible popup card.
pub fn close_all() {
    send_command(Command::CloseAll);
}

fn send_command(cmd: Command) {
    let Some(mgr) = MANAGER.get_or_init(start_manager).as_ref() else {
        return;
    };
    mgr.queue
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .push_back(cmd);
    // SAFETY: `ctrl_hwnd` is a live message-only window owned by the popup thread.
    unsafe {
        let _ = PostMessageW(
            Some(HWND(mgr.ctrl_hwnd as *mut c_void)),
            WM_APP_CMD,
            WPARAM(0),
            LPARAM(0),
        );
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

fn colorref(c: egui::Color32) -> COLORREF {
    COLORREF(u32::from(c.r()) | (u32::from(c.g()) << 8) | (u32::from(c.b()) << 16))
}

fn blend_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |ca: u8, cb: u8| -> u8 {
        ((f32::from(ca) * (1.0 - t)) + (f32::from(cb) * t))
            .round()
            .clamp(0.0, 255.0) as u8
    };
    egui::Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

fn start_manager() -> Option<Manager> {
    let queue = Arc::new(Mutex::new(VecDeque::new()));
    let queue_thread = Arc::clone(&queue);
    let (tx, rx) = mpsc::channel::<Option<isize>>();
    let spawned = std::thread::Builder::new()
        .name("notify-popup".into())
        .spawn(move || run_loop(queue_thread, tx));
    if let Err(err) = spawned {
        log::debug!("could not start Windows popup notification thread: {err}");
        return None;
    }
    let ctrl_hwnd = rx.recv().ok().flatten()?;
    Some(Manager { queue, ctrl_hwnd })
}

#[derive(Default)]
struct PopupHost {
    queue: Option<Arc<Mutex<VecDeque<Command>>>>,
    hinstance: isize,
    toast_class: Vec<u16>,
    toasts: Vec<isize>,
    app_icon_cache: Option<(u32, Vec<u8>)>,
}

thread_local! {
    static HOST: RefCell<PopupHost> = RefCell::new(PopupHost::default());
}

fn cached_app_icon_rgba(px: u32) -> Vec<u8> {
    let px = px.max(12);
    HOST.with(|host| {
        let mut host = host.borrow_mut();
        if let Some((cached_px, rgba)) = &host.app_icon_cache
            && *cached_px == px
        {
            return rgba.clone();
        }
        let rgba = crate::util::app_icon_rgba(px as usize);
        host.app_icon_cache = Some((px, rgba.clone()));
        rgba
    })
}

enum Phase {
    Entering {
        started: Instant,
    },
    Visible {
        remaining: Duration,
        last_tick: Instant,
    },
    Leaving {
        started: Instant,
    },
}

struct ToastState {
    id: u64,
    title: String,
    body: String,
    app_icon_rgba: Vec<u8>,
    app_icon_px: u32,
    avatar_rgba: Vec<u8>,
    avatar_px: u32,
    config: PopupConfig,
    target: NotificationTarget,
    opened: Arc<Mutex<Vec<NotificationTarget>>>,
    wake: Arc<dyn Fn() + Send + Sync>,
    total_duration: Duration,
    dpi_scale: f32,
    width_px: i32,
    height_px: i32,
    base_rect: RECT,
    current_rect: RECT,
    created: Instant,
    phase: Phase,
    hovered: bool,
    close_hovered: bool,
    tracking_leave: bool,
    current_alpha: u8,
    uses_dwm_round: bool,
}

fn run_loop(queue: Arc<Mutex<VecDeque<Command>>>, ready: mpsc::Sender<Option<isize>>) {
    // SAFETY: Win32 thread initialization; all windows created below belong to this thread.
    unsafe {
        let _ = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let hmodule = match GetModuleHandleW(None) {
            Ok(h) => h,
            Err(err) => {
                log::debug!("GetModuleHandleW failed for popup thread: {err}");
                let _ = ready.send(None);
                return;
            }
        };
        let hinstance = HINSTANCE(hmodule.0);

        let ctrl_class = wide("SuppNotifyController");
        let wc_ctrl = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(controller_wnd_proc),
            hInstance: hinstance,
            lpszClassName: PCWSTR(ctrl_class.as_ptr()),
            ..Default::default()
        };
        if RegisterClassExW(&wc_ctrl) == 0 {
            log::debug!("RegisterClassExW controller failed");
            let _ = ready.send(None);
            return;
        }

        let toast_class = wide("SuppNotifyToast");
        let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
        let wc_toast = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(toast_wnd_proc),
            hInstance: hinstance,
            hCursor: cursor,
            lpszClassName: PCWSTR(toast_class.as_ptr()),
            ..Default::default()
        };
        if RegisterClassExW(&wc_toast) == 0 {
            log::debug!("RegisterClassExW toast failed");
            let _ = ready.send(None);
            return;
        }

        let ctrl_name = wide("SuppNotifyControllerWindow");
        let ctrl_hwnd = match CreateWindowExW(
            Default::default(),
            PCWSTR(ctrl_class.as_ptr()),
            PCWSTR(ctrl_name.as_ptr()),
            Default::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinstance),
            None,
        ) {
            Ok(hwnd) => hwnd,
            Err(err) => {
                log::debug!("CreateWindowExW controller failed: {err}");
                let _ = ready.send(None);
                return;
            }
        };

        HOST.with(|host| {
            let mut host = host.borrow_mut();
            host.queue = Some(queue);
            host.hinstance = hinstance.0 as isize;
            host.toast_class = toast_class;
        });

        let _ = ready.send(Some(ctrl_hwnd.0 as isize));

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe extern "system" fn controller_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_APP_CMD {
        drain_commands();
        return LRESULT(0);
    }
    // SAFETY: Standard default window procedure forwarding.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn drain_commands() {
    let cmds: Vec<Command> = HOST.with(|host| {
        let host = host.borrow();
        let Some(queue) = host.queue.as_ref() else {
            return Vec::new();
        };
        let mut q = queue.lock().unwrap_or_else(|p| p.into_inner());
        q.drain(..).collect()
    });
    for cmd in cmds {
        match cmd {
            Command::Show(req) => spawn_toast(req),
            Command::Close(id) => close_toast_by_id(id),
            Command::CloseAll => close_all_toasts(),
        }
    }
}

fn primary_work_area() -> RECT {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 1280,
        bottom: 720,
    };
    // SAFETY: `rect` is a valid out-pointer for `SPI_GETWORKAREA`.
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some((&raw mut rect).cast::<c_void>()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    rect
}

/// Computes the physical target rectangle for a toast at `slot` (0 = closest to anchor corner),
/// sitting cleanly inside `work_area` for any [`NotificationCorner`] and [`NotificationSize`].
pub(crate) fn stack_rect(
    work_area: RECT,
    slot: usize,
    dpi_scale: f32,
    corner: NotificationCorner,
    size: NotificationSize,
) -> RECT {
    let (cw, ch) = size.dimensions();
    let w = (cw * dpi_scale).round() as i32;
    let h = (ch * dpi_scale).round() as i32;
    let margin = (MARGIN_EDGE * dpi_scale).round() as i32;
    let gap = (STACK_GAP * dpi_scale).round() as i32;
    let offset = (slot as i32) * (h + gap);

    let (left, right) = match corner {
        NotificationCorner::BottomRight | NotificationCorner::TopRight => {
            let right = work_area.right - margin;
            (right - w, right)
        }
        NotificationCorner::BottomLeft | NotificationCorner::TopLeft => {
            let left = work_area.left + margin;
            (left, left + w)
        }
    };

    let (top, bottom) = match corner {
        NotificationCorner::BottomRight | NotificationCorner::BottomLeft => {
            let bottom = work_area.bottom - margin - offset;
            (bottom - h, bottom)
        }
        NotificationCorner::TopRight | NotificationCorner::TopLeft => {
            let top = work_area.top + margin + offset;
            (top, top + h)
        }
    };

    RECT {
        left,
        top,
        right,
        bottom,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EffectFrame {
    pub rect: RECT,
    pub alpha: u8,
    pub accent_glow: f32,
}

fn entrance_duration(effect: NotificationEffect) -> Duration {
    match effect {
        NotificationEffect::Spring => Duration::from_millis(460),
        NotificationEffect::Bloom => Duration::from_millis(420),
        NotificationEffect::Glide => Duration::from_millis(380),
        NotificationEffect::Fade => Duration::from_millis(340),
    }
}

/// Easing curves for the four customizable popup entry animations.
pub(crate) fn ease_effect(effect: NotificationEffect, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match effect {
        NotificationEffect::Spring => {
            // Back-out overshoot (~1.065 peak before settling cleanly to 1.0).
            const C1: f32 = 1.15;
            const C3: f32 = C1 + 1.0;
            let u = t - 1.0;
            1.0 + C3 * u * u * u + C1 * u * u
        }
        NotificationEffect::Bloom => {
            // Snappy exponential ease-out.
            if t >= 1.0 {
                1.0
            } else {
                1.0 - 2.0_f32.powf(-9.0 * t)
            }
        }
        NotificationEffect::Glide => {
            let inv = 1.0 - t;
            1.0 - inv.powi(5)
        }
        NotificationEffect::Fade => {
            let inv = 1.0 - t;
            1.0 - inv * inv * inv
        }
    }
}

/// Computes animated geometry, alpha, and border glow for normalized entry progress `t` in `0.0..=1.0`.
pub(crate) fn compute_entry_frame(
    base: RECT,
    corner: NotificationCorner,
    effect: NotificationEffect,
    dpi_scale: f32,
    max_alpha: u8,
    t: f32,
) -> EffectFrame {
    let t = t.clamp(0.0, 1.0);
    let eased = ease_effect(effect, t);
    let bw = (base.right - base.left).max(1);
    let bh = (base.bottom - base.top).max(1);

    let alpha_t = match effect {
        NotificationEffect::Spring | NotificationEffect::Glide => (t / 0.45).clamp(0.0, 1.0),
        NotificationEffect::Bloom => (t / 0.55).clamp(0.0, 1.0),
        NotificationEffect::Fade => eased,
    };
    let alpha = (alpha_t * f32::from(max_alpha)).round().clamp(0.0, 255.0) as u8;
    let accent_glow = (1.0 - t).powi(2);

    let rect = match effect {
        NotificationEffect::Spring => {
            let travel = (34.0 * dpi_scale) * (1.0 - eased);
            let dy = match corner {
                NotificationCorner::BottomRight | NotificationCorner::BottomLeft => {
                    travel.round() as i32
                }
                NotificationCorner::TopRight | NotificationCorner::TopLeft => {
                    -(travel.round() as i32)
                }
            };
            RECT {
                left: base.left,
                top: base.top + dy,
                right: base.right,
                bottom: base.bottom + dy,
            }
        }
        NotificationEffect::Bloom => {
            let scale = (0.76 + 0.24 * eased).clamp(0.70, 1.0);
            let w = ((bw as f32) * scale).round().max(1.0) as i32;
            let h = ((bh as f32) * scale).round().max(1.0) as i32;
            let (left, right) = match corner {
                NotificationCorner::BottomRight | NotificationCorner::TopRight => {
                    (base.right - w, base.right)
                }
                NotificationCorner::BottomLeft | NotificationCorner::TopLeft => {
                    (base.left, base.left + w)
                }
            };
            let (top, bottom) = match corner {
                NotificationCorner::BottomRight | NotificationCorner::BottomLeft => {
                    (base.bottom - h, base.bottom)
                }
                NotificationCorner::TopRight | NotificationCorner::TopLeft => {
                    (base.top, base.top + h)
                }
            };
            RECT {
                left,
                top,
                right,
                bottom,
            }
        }
        NotificationEffect::Glide => {
            let travel = (48.0 * dpi_scale) * (1.0 - eased);
            let dx = match corner {
                NotificationCorner::BottomRight | NotificationCorner::TopRight => {
                    travel.round() as i32
                }
                NotificationCorner::BottomLeft | NotificationCorner::TopLeft => {
                    -(travel.round() as i32)
                }
            };
            RECT {
                left: base.left + dx,
                top: base.top,
                right: base.right + dx,
                bottom: base.bottom,
            }
        }
        NotificationEffect::Fade => {
            let drift = (8.0 * dpi_scale) * (1.0 - eased);
            let dy = match corner {
                NotificationCorner::BottomRight | NotificationCorner::BottomLeft => {
                    drift.round() as i32
                }
                NotificationCorner::TopRight | NotificationCorner::TopLeft => {
                    -(drift.round() as i32)
                }
            };
            RECT {
                left: base.left,
                top: base.top + dy,
                right: base.right,
                bottom: base.bottom + dy,
            }
        }
    };

    EffectFrame {
        rect,
        alpha,
        accent_glow,
    }
}

/// Remaining countdown fraction in `0.0..=1.0`.
pub(crate) fn remaining_fraction(remaining: Duration, total: Duration) -> f32 {
    let total_secs = total.as_secs_f32();
    if total_secs <= f32::EPSILON {
        0.0
    } else {
        (remaining.as_secs_f32() / total_secs).clamp(0.0, 1.0)
    }
}

fn close_rect_px(width_px: i32, dpi_scale: f32) -> RECT {
    let s = |dp: f32| (dp * dpi_scale).round() as i32;
    let size = s(CLOSE_SIZE).max(16);
    let header_h = s(HEADER_HEIGHT).max(28);
    let right = width_px - s(CLOSE_RIGHT_INSET);
    let top = ((header_h - size) / 2).max(2);
    RECT {
        left: right - size,
        top,
        right,
        bottom: top + size,
    }
}

fn pt_in_rect(rc: RECT, pt: POINT) -> bool {
    pt.x >= rc.left && pt.x < rc.right && pt.y >= rc.top && pt.y < rc.bottom
}

fn avatar_dip(size: NotificationSize) -> f32 {
    match size {
        NotificationSize::Compact => 48.0,
        NotificationSize::Normal => 56.0,
        NotificationSize::Large => 64.0,
    }
}

fn spawn_toast(req: PopupRequest) {
    let (hinstance, toast_class_ptr) = HOST.with(|host| {
        let host = host.borrow();
        (
            HINSTANCE(host.hinstance as *mut c_void),
            host.toast_class.as_ptr(),
        )
    });
    if hinstance.0.is_null() || toast_class_ptr.is_null() {
        return;
    }

    let config = req.config;
    let work_area = primary_work_area();
    let init_rect = stack_rect(work_area, 0, 1.0, config.corner, config.size);
    let title_wide = wide(&req.title);

    // SAFETY: `toast_class_ptr` and `title_wide` are NUL-terminated for the duration of the call.
    let hwnd = unsafe {
        match CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
            PCWSTR(toast_class_ptr),
            PCWSTR(title_wide.as_ptr()),
            WS_POPUP,
            init_rect.left,
            init_rect.top,
            init_rect.right - init_rect.left,
            init_rect.bottom - init_rect.top,
            None,
            None,
            Some(hinstance),
            None,
        ) {
            Ok(h) => h,
            Err(err) => {
                log::debug!("CreateWindowExW toast failed: {err}");
                return;
            }
        }
    };

    // SAFETY: `hwnd` is a live window created on this thread.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let dpi_scale = if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 };

    let base_rect = stack_rect(work_area, 0, dpi_scale, config.corner, config.size);
    let width_px = base_rect.right - base_rect.left;
    let height_px = base_rect.bottom - base_rect.top;

    let total_secs = if req.is_call {
        config.duration_secs.max(30)
    } else {
        config.duration_secs.clamp(2, 30)
    };
    let total_duration = Duration::from_secs(u64::from(total_secs));

    let app_icon_px = (APP_ICON_SIZE * dpi_scale).round().max(14.0) as u32;
    let app_icon_rgba = cached_app_icon_rgba(app_icon_px);

    let avatar_px = (avatar_dip(config.size) * dpi_scale).round().max(24.0) as u32;
    let avatar_rgba = rasterize_ringed_avatar(
        req.picture.as_deref(),
        &req.title,
        avatar_px,
        config.palette,
    );

    let start_frame = compute_entry_frame(
        base_rect,
        config.corner,
        config.effect,
        dpi_scale,
        config.max_alpha(),
        0.0,
    );

    let now = Instant::now();
    // SAFETY: `hwnd` is alive on this thread.
    let uses_dwm_round = unsafe {
        apply_window_chrome(
            hwnd,
            start_frame.rect.right - start_frame.rect.left,
            start_frame.rect.bottom - start_frame.rect.top,
            dpi_scale,
            &config.palette,
        )
    };

    let state = Box::new(ToastState {
        id: req.id,
        title: req.title,
        body: req.body,
        app_icon_rgba,
        app_icon_px,
        avatar_rgba,
        avatar_px,
        config,
        target: req.target,
        opened: req.opened,
        wake: req.wake,
        total_duration,
        dpi_scale,
        width_px,
        height_px,
        base_rect,
        current_rect: start_frame.rect,
        created: now,
        phase: Phase::Entering { started: now },
        hovered: false,
        close_hovered: false,
        tracking_leave: false,
        current_alpha: start_frame.alpha,
        uses_dwm_round,
    });
    let state_ptr = Box::into_raw(state);

    // SAFETY: `hwnd` is valid and belongs to this thread; `state_ptr` is freed on `WM_NCDESTROY`.
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), start_frame.alpha, LWA_ALPHA);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            start_frame.rect.left,
            start_frame.rect.top,
            start_frame.rect.right - start_frame.rect.left,
            start_frame.rect.bottom - start_frame.rect.top,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = InvalidateRect(Some(hwnd), None, false);
        SetTimer(Some(hwnd), TIMER_TICK, TICK_MS, None);
    }

    // Insert newest toast at slot 0 (anchor corner) and shift older ones outward.
    let evicted: Vec<isize> = HOST.with(|host| {
        let mut host = host.borrow_mut();
        host.toasts.insert(0, hwnd.0 as isize);
        let mut drop_list = Vec::new();
        while host.toasts.len() > MAX_VISIBLE {
            if let Some(old) = host.toasts.pop() {
                drop_list.push(old);
            }
        }
        drop_list
    });
    restack_toasts();
    for old_hwnd in evicted {
        // SAFETY: `old_hwnd` belongs to this thread.
        unsafe {
            let _ = DestroyWindow(HWND(old_hwnd as *mut c_void));
        }
    }
}

unsafe fn apply_window_chrome(
    hwnd: HWND,
    width_px: i32,
    height_px: i32,
    dpi_scale: f32,
    palette: &Palette,
) -> bool {
    let dark = BOOL::from(palette.dark);
    // SAFETY: `hwnd` is a live window on this thread and attribute payloads match DWM sizes.
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&raw const dark).cast(),
            size_of::<BOOL>() as u32,
        );
        let border = colorref(palette.outline);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            (&raw const border).cast(),
            size_of::<COLORREF>() as u32,
        );
        let corner = DWMWCP_ROUND;
        let rounded = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&raw const corner).cast(),
            size_of_val(&corner) as u32,
        );
        if rounded.is_ok() {
            true
        } else {
            apply_round_region(hwnd, width_px, height_px, dpi_scale);
            false
        }
    }
}

unsafe fn apply_round_region(hwnd: HWND, width_px: i32, height_px: i32, dpi_scale: f32) {
    let radius = (CORNER_RADIUS * dpi_scale).round() as i32;
    // SAFETY: Creates and assigns an HRGN to `hwnd`; Windows takes ownership on success.
    unsafe {
        let rgn = CreateRoundRectRgn(0, 0, width_px + 1, height_px + 1, radius * 2, radius * 2);
        if !rgn.is_invalid() && SetWindowRgn(hwnd, Some(rgn), true) == 0 {
            let _ = DeleteObject(HGDIOBJ(rgn.0));
        }
    }
}

fn restack_toasts() {
    let work_area = primary_work_area();
    let handles: Vec<isize> = HOST.with(|host| host.borrow().toasts.clone());
    let now = Instant::now();
    for (slot, raw_hwnd) in handles.into_iter().enumerate() {
        let hwnd = HWND(raw_hwnd as *mut c_void);
        // SAFETY: `hwnd` lives on this thread; `GWLP_USERDATA` is `ToastState`.
        unsafe {
            let Some(state) = toast_state_mut(hwnd) else {
                continue;
            };
            let new_rect = stack_rect(
                work_area,
                slot,
                state.dpi_scale,
                state.config.corner,
                state.config.size,
            );
            if new_rect.left != state.base_rect.left || new_rect.top != state.base_rect.top {
                state.base_rect = new_rect;
                let frame = current_frame_rect(state, now);
                state.current_rect = frame;
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    frame.left,
                    frame.top,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOSIZE,
                );
            }
        }
    }
}

fn current_frame_rect(state: &ToastState, now: Instant) -> RECT {
    match state.phase {
        Phase::Entering { started } => {
            let dur = entrance_duration(state.config.effect).as_secs_f32();
            let t = (now.saturating_duration_since(started).as_secs_f32() / dur).clamp(0.0, 1.0);
            compute_entry_frame(
                state.base_rect,
                state.config.corner,
                state.config.effect,
                state.dpi_scale,
                state.config.max_alpha(),
                t,
            )
            .rect
        }
        Phase::Visible { .. } | Phase::Leaving { .. } => state.base_rect,
    }
}

fn close_toast_by_id(id: u64) {
    let handles: Vec<isize> = HOST.with(|host| host.borrow().toasts.clone());
    for raw_hwnd in handles {
        let hwnd = HWND(raw_hwnd as *mut c_void);
        // SAFETY: `hwnd` lives on this thread.
        unsafe {
            if let Some(state) = toast_state_mut(hwnd)
                && state.id == id
            {
                begin_leave(hwnd, state);
                break;
            }
        }
    }
}

fn close_all_toasts() {
    let handles: Vec<isize> = HOST.with(|host| host.borrow().toasts.clone());
    for raw_hwnd in handles {
        // SAFETY: `raw_hwnd` belongs to this thread.
        unsafe {
            let _ = DestroyWindow(HWND(raw_hwnd as *mut c_void));
        }
    }
}

fn begin_leave(hwnd: HWND, state: &mut ToastState) {
    if !matches!(state.phase, Phase::Leaving { .. }) {
        state.phase = Phase::Leaving {
            started: Instant::now(),
        };
        // SAFETY: `hwnd` is alive on this thread.
        unsafe {
            SetTimer(Some(hwnd), TIMER_TICK, TICK_MS, None);
        }
    }
}

unsafe fn toast_state_mut<'a>(hwnd: HWND) -> Option<&'a mut ToastState> {
    // SAFETY: Caller ensures `hwnd` lives on the popup thread and `GWLP_USERDATA` points to `ToastState`.
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut ToastState;
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { &mut *ptr })
    }
}

unsafe extern "system" fn toast_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            // SAFETY: Standard Win32 paint cycle.
            unsafe {
                if let Some(state) = toast_state_mut(hwnd) {
                    paint_toast(hwnd, state);
                    return LRESULT(0);
                }
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_TIMER if wparam.0 == TIMER_TICK => {
            // SAFETY: `hwnd` is alive on this thread.
            unsafe {
                if let Some(state) = toast_state_mut(hwnd) {
                    tick_toast(hwnd, state);
                }
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            // SAFETY: `hwnd` is alive on this thread.
            unsafe {
                if let Some(state) = toast_state_mut(hwnd) {
                    on_mouse_move(hwnd, state);
                }
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            // SAFETY: `hwnd` is alive on this thread.
            unsafe {
                if let Some(state) = toast_state_mut(hwnd) {
                    state.hovered = false;
                    state.close_hovered = false;
                    state.tracking_leave = false;
                    if let Phase::Visible {
                        remaining,
                        last_tick,
                    } = &mut state.phase
                    {
                        *last_tick = Instant::now();
                        if *remaining < RESUME_MIN_REMAINING {
                            *remaining = RESUME_MIN_REMAINING;
                        }
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            // SAFETY: `hwnd` is alive on this thread.
            unsafe {
                if let Some(state) = toast_state_mut(hwnd) {
                    let close_clicked = cursor_in_close(hwnd, state);
                    if !close_clicked {
                        state
                            .opened
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .push(state.target.clone());
                        (state.wake)();
                    }
                    begin_leave(hwnd, state);
                }
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            // SAFETY: `hwnd` is alive on this thread.
            unsafe {
                if let Some(state) = toast_state_mut(hwnd) {
                    begin_leave(hwnd, state);
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            // SAFETY: `hwnd` is being destroyed on this thread.
            unsafe {
                let _ = KillTimer(Some(hwnd), TIMER_TICK);
            }
            let raw = hwnd.0 as isize;
            HOST.with(|host| {
                host.borrow_mut().toasts.retain(|h| *h != raw);
            });
            restack_toasts();
            LRESULT(0)
        }
        WM_NCDESTROY => {
            // SAFETY: Reclaims the boxed `ToastState` allocated in `spawn_toast`.
            unsafe {
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ToastState;
                if !ptr.is_null() {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    drop(Box::from_raw(ptr));
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

unsafe fn cursor_in_close(hwnd: HWND, state: &ToastState) -> bool {
    let mut pt = POINT::default();
    // SAFETY: `pt` is valid and `hwnd` is live.
    unsafe {
        if GetCursorPos(&mut pt).is_err() {
            return false;
        }
        let _ = ScreenToClient(hwnd, &mut pt);
    }
    pt_in_rect(close_rect_px(state.width_px, state.dpi_scale), pt)
}

unsafe fn on_mouse_move(hwnd: HWND, state: &mut ToastState) {
    if !state.tracking_leave {
        let mut tme = TRACKMOUSEEVENT {
            cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: hwnd,
            dwHoverTime: 0,
        };
        // SAFETY: `tme` points to a valid `TRACKMOUSEEVENT` for `hwnd`.
        if unsafe { TrackMouseEvent(&mut tme) }.is_ok() {
            state.tracking_leave = true;
        }
    }
    let close_now = unsafe { cursor_in_close(hwnd, state) };
    let changed = !state.hovered || state.close_hovered != close_now;
    state.hovered = true;
    state.close_hovered = close_now;
    // SAFETY: `LoadCursorW` and `SetCursor` use built-in system cursor IDs.
    unsafe {
        let cursor_id = if close_now { IDC_ARROW } else { IDC_HAND };
        if let Ok(cursor) = LoadCursorW(None, cursor_id) {
            SetCursor(Some(cursor));
        }
        if changed {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
}

unsafe fn tick_toast(hwnd: HWND, state: &mut ToastState) {
    let now = Instant::now();
    let max_alpha = state.config.max_alpha();
    let shimmer_active =
        now.saturating_duration_since(state.created).as_millis() <= u128::from(SHIMMER_DURATION_MS);

    match state.phase {
        Phase::Entering { started } => {
            let dur = entrance_duration(state.config.effect).as_secs_f32();
            let t = (now.saturating_duration_since(started).as_secs_f32() / dur).clamp(0.0, 1.0);
            let frame = compute_entry_frame(
                state.base_rect,
                state.config.corner,
                state.config.effect,
                state.dpi_scale,
                max_alpha,
                t,
            );
            let fw = (frame.rect.right - frame.rect.left).max(1);
            let fh = (frame.rect.bottom - frame.rect.top).max(1);
            let prev_w = state.current_rect.right - state.current_rect.left;
            let prev_h = state.current_rect.bottom - state.current_rect.top;
            let size_changed = fw != prev_w || fh != prev_h;
            let pos_changed = frame.rect.left != state.current_rect.left
                || frame.rect.top != state.current_rect.top;
            state.current_rect = frame.rect;

            // SAFETY: `hwnd` is a live layered window on this thread.
            unsafe {
                if frame.alpha != state.current_alpha {
                    state.current_alpha = frame.alpha;
                    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), frame.alpha, LWA_ALPHA);
                }
                if size_changed || pos_changed {
                    let mut flags = SWP_NOACTIVATE | SWP_NOZORDER;
                    if !size_changed {
                        flags |= SWP_NOSIZE;
                    }
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        frame.rect.left,
                        frame.rect.top,
                        fw,
                        fh,
                        flags,
                    );
                    if size_changed && !state.uses_dwm_round {
                        apply_round_region(hwnd, fw, fh, state.dpi_scale);
                    }
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
            }

            if t >= 1.0 {
                state.current_rect = state.base_rect;
                // SAFETY: Snap cleanly onto base_rect and full max_alpha.
                unsafe {
                    if state.current_alpha != max_alpha {
                        state.current_alpha = max_alpha;
                        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), max_alpha, LWA_ALPHA);
                    }
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        state.base_rect.left,
                        state.base_rect.top,
                        state.width_px,
                        state.height_px,
                        SWP_NOACTIVATE | SWP_NOZORDER,
                    );
                    if !state.uses_dwm_round {
                        apply_round_region(hwnd, state.width_px, state.height_px, state.dpi_scale);
                    }
                }
                state.phase = Phase::Visible {
                    remaining: state.total_duration,
                    last_tick: now,
                };
            }
        }
        Phase::Visible {
            ref mut remaining,
            ref mut last_tick,
        } => {
            let dt = now.saturating_duration_since(*last_tick);
            *last_tick = now;
            if !state.hovered {
                *remaining = remaining.saturating_sub(dt);
                if remaining.is_zero() {
                    begin_leave(hwnd, state);
                    return;
                }
                if state.config.show_progress_bar || shimmer_active {
                    // SAFETY: Keep countdown bar & entry shimmer animating smoothly.
                    unsafe {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
            } else if shimmer_active {
                // SAFETY: Finish entry shimmer even if hovered immediately.
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
        }
        Phase::Leaving { started } => {
            let elapsed = now.saturating_duration_since(started);
            let t = (elapsed.as_secs_f32() / LEAVE_DURATION.as_secs_f32()).clamp(0.0, 1.0);
            let alpha = ((1.0 - t) * f32::from(max_alpha)).round().clamp(0.0, 255.0) as u8;
            // SAFETY: `hwnd` is a live layered window on this thread.
            unsafe {
                if alpha != state.current_alpha {
                    state.current_alpha = alpha;
                    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
                }
            }
            if t >= 1.0 {
                // SAFETY: Destroying the completed toast window on this thread.
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            }
        }
    }
}

unsafe fn paint_toast(hwnd: HWND, state: &ToastState) {
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: Paired `BeginPaint` / `EndPaint` with an offscreen double buffer.
    unsafe {
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut client = RECT::default();
        if GetClientRect(hwnd, &mut client).is_err() {
            let _ = EndPaint(hwnd, &ps);
            return;
        }
        let w = client.right - client.left;
        let h = client.bottom - client.top;
        if w <= 0 || h <= 0 {
            let _ = EndPaint(hwnd, &ps);
            return;
        }

        let mem_dc = CreateCompatibleDC(Some(hdc));
        let bmp = CreateCompatibleBitmap(hdc, w, h);
        if mem_dc.is_invalid() || bmp.is_invalid() {
            if !bmp.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(bmp.0));
            }
            if !mem_dc.is_invalid() {
                let _ = DeleteDC(mem_dc);
            }
            let _ = EndPaint(hwnd, &ps);
            return;
        }

        let old_bmp = SelectObject(mem_dc, HGDIOBJ(bmp.0));
        let palette = state.config.palette;
        let scale = state.dpi_scale;
        let s = |dp: f32| (dp * scale).round() as i32;

        let bg_color = if state.hovered {
            palette.surface_hover
        } else {
            palette.surface
        };
        let bg_brush = CreateSolidBrush(colorref(bg_color));
        FillRect(mem_dc, &client, bg_brush);
        let _ = DeleteObject(HGDIOBJ(bg_brush.0));

        // Header strip with Supp icon, "Supp" wordmark, live timer, and close button.
        let header_h = s(HEADER_HEIGHT).clamp(24, h.saturating_sub(40).max(24));
        let header_bg = blend_color(bg_color, palette.accent, 0.08);
        let header_rect = RECT {
            left: 0,
            top: 0,
            right: w,
            bottom: header_h,
        };
        let header_brush = CreateSolidBrush(colorref(header_bg));
        FillRect(mem_dc, &header_rect, header_brush);
        let _ = DeleteObject(HGDIOBJ(header_brush.0));

        // Top luminous accent bar + entry shimmer sweep.
        let now = Instant::now();
        let shimmer_t = (now.saturating_duration_since(state.created).as_secs_f32()
            / (SHIMMER_DURATION_MS as f32 / 1000.0))
            .clamp(0.0, 1.0);
        let accent_bar_h = s(3.0).max(2);
        let base_bar_color = blend_color(palette.accent, bg_color, 0.25);
        let top_bar_rc = RECT {
            left: 0,
            top: 0,
            right: w,
            bottom: accent_bar_h,
        };
        let top_bar_brush = CreateSolidBrush(colorref(base_bar_color));
        FillRect(mem_dc, &top_bar_rc, top_bar_brush);
        let _ = DeleteObject(HGDIOBJ(top_bar_brush.0));

        if shimmer_t < 1.0 {
            let sweep_w = (w as f32 * 0.42).round() as i32;
            let cx = (-sweep_w) + (((w + sweep_w * 2) as f32) * shimmer_t).round() as i32;
            let hi_rc = RECT {
                left: (cx - sweep_w / 2).max(0),
                top: 0,
                right: (cx + sweep_w / 2).min(w),
                bottom: accent_bar_h,
            };
            if hi_rc.right > hi_rc.left {
                let hi_color = blend_color(palette.accent, egui::Color32::WHITE, 0.65);
                let hi_brush = CreateSolidBrush(colorref(hi_color));
                FillRect(mem_dc, &hi_rc, hi_brush);
                let _ = DeleteObject(HGDIOBJ(hi_brush.0));
            }
        }

        // Hairline divider beneath header.
        let divider_color = blend_color(palette.outline, bg_color, 0.35);
        let divider_rc = RECT {
            left: 0,
            top: header_h - 1,
            right: w,
            bottom: header_h,
        };
        let divider_brush = CreateSolidBrush(colorref(divider_color));
        FillRect(mem_dc, &divider_rc, divider_brush);
        let _ = DeleteObject(HGDIOBJ(divider_brush.0));

        // Supp app icon inside header left.
        let icon_x = s(12.0);
        let icon_y = ((header_h - state.app_icon_px as i32) / 2).max(2);
        blit_rgba_over_bg(
            mem_dc,
            icon_x,
            icon_y,
            state.app_icon_px,
            &state.app_icon_rgba,
            header_bg,
        );

        let _ = SetBkMode(mem_dc, TRANSPARENT);

        // Close button (`×`) in header top-right.
        let close_rc = close_rect_px(w, scale);
        if state.close_hovered {
            let hover_brush = CreateSolidBrush(colorref(palette.surface_active));
            let old_brush = SelectObject(mem_dc, HGDIOBJ(hover_brush.0));
            let old_pen = SelectObject(mem_dc, GetStockObject(NULL_PEN));
            let _ = RoundRect(
                mem_dc,
                close_rc.left,
                close_rc.top,
                close_rc.right,
                close_rc.bottom,
                s(10.0),
                s(10.0),
            );
            SelectObject(mem_dc, old_pen);
            SelectObject(mem_dc, old_brush);
            let _ = DeleteObject(HGDIOBJ(hover_brush.0));
        }
        let close_font = make_font(s(14.0).max(11), FW_NORMAL.0 as i32);
        let old_font = SelectObject(mem_dc, HGDIOBJ(close_font.0));
        let close_color = if state.close_hovered {
            palette.text
        } else {
            palette.dim
        };
        let _ = SetTextColor(mem_dc, colorref(close_color));
        let mut close_text: Vec<u16> = "×".encode_utf16().collect();
        let mut close_draw_rc = close_rc;
        DrawTextW(
            mem_dc,
            &mut close_text,
            &mut close_draw_rc,
            DT_SINGLELINE | DT_CENTER | DT_VCENTER | DT_NOPREFIX,
        );
        SelectObject(mem_dc, old_font);
        let _ = DeleteObject(HGDIOBJ(close_font.0));

        // Header remaining time readout beside `×`.
        let (remaining, progress) = match state.phase {
            Phase::Entering { .. } => (state.total_duration, 1.0),
            Phase::Visible { remaining, .. } => {
                (remaining, remaining_fraction(remaining, state.total_duration))
            }
            Phase::Leaving { .. } => (Duration::ZERO, 0.0),
        };
        let secs_left = remaining.as_secs_f32().ceil().max(0.0) as u32;
        let timer_label = if state.hovered {
            format!("‖ {secs_left}s")
        } else {
            format!("{secs_left}s")
        };
        let timer_w = s(44.0);
        let timer_rc = RECT {
            left: (close_rc.left - timer_w - s(4.0)).max(icon_x + state.app_icon_px as i32 + s(32.0)),
            top: accent_bar_h,
            right: close_rc.left - s(4.0),
            bottom: header_h - 1,
        };
        if timer_rc.right > timer_rc.left {
            let timer_font = make_font(s(11.0).max(10), FW_NORMAL.0 as i32);
            let old_font = SelectObject(mem_dc, HGDIOBJ(timer_font.0));
            let _ = SetTextColor(mem_dc, colorref(palette.dim));
            let mut timer_utf16: Vec<u16> = timer_label.encode_utf16().collect();
            let mut trc = timer_rc;
            DrawTextW(
                mem_dc,
                &mut timer_utf16,
                &mut trc,
                DT_SINGLELINE | DT_RIGHT | DT_VCENTER | DT_NOPREFIX,
            );
            SelectObject(mem_dc, old_font);
            let _ = DeleteObject(HGDIOBJ(timer_font.0));
        }

        // "Supp" label beside icon in header.
        let brand_rc = RECT {
            left: icon_x + state.app_icon_px as i32 + s(7.0),
            top: accent_bar_h,
            right: timer_rc.left - s(4.0),
            bottom: header_h - 1,
        };
        if brand_rc.right > brand_rc.left {
            let brand_font = make_font(s(12.5).max(11), FW_SEMIBOLD.0 as i32);
            let old_font = SelectObject(mem_dc, HGDIOBJ(brand_font.0));
            let _ = SetTextColor(mem_dc, colorref(palette.text));
            let mut brand_utf16: Vec<u16> = "Supp".encode_utf16().collect();
            let mut brc = brand_rc;
            DrawTextW(
                mem_dc,
                &mut brand_utf16,
                &mut brc,
                DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
            );
            SelectObject(mem_dc, old_font);
            let _ = DeleteObject(HGDIOBJ(brand_font.0));
        }

        // Square body: centered ringed avatar, sender/chat title, message body.
        let progress_reserve = if state.config.show_progress_bar {
            s(PROGRESS_BOTTOM_INSET + PROGRESS_HEIGHT + 9.0)
        } else {
            s(10.0)
        };
        let body_bottom_limit = (h - progress_reserve).max(header_h + 24);
        let pad_x = s(14.0);
        let avatar_x = ((w - state.avatar_px as i32) / 2).max(pad_x);
        let avatar_y = header_h + s(10.0);
        if avatar_y + (state.avatar_px as i32) < body_bottom_limit {
            blit_rgba_over_bg(
                mem_dc,
                avatar_x,
                avatar_y,
                state.avatar_px,
                &state.avatar_rgba,
                bg_color,
            );
        }

        let title_top = avatar_y + state.avatar_px as i32 + s(7.0);
        let title_h = s(20.0).max(16);
        if w > pad_x * 2 && title_top < body_bottom_limit {
            let title_font = make_font(s(14.5).max(12), FW_BOLD.0 as i32);
            let old_font = SelectObject(mem_dc, HGDIOBJ(title_font.0));
            let _ = SetTextColor(mem_dc, colorref(palette.text));
            let mut title_rc = RECT {
                left: pad_x,
                top: title_top,
                right: w - pad_x,
                bottom: (title_top + title_h).min(body_bottom_limit),
            };
            let mut title_utf16: Vec<u16> = state.title.encode_utf16().collect();
            if !title_utf16.is_empty() {
                DrawTextW(
                    mem_dc,
                    &mut title_utf16,
                    &mut title_rc,
                    DT_SINGLELINE | DT_CENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
                );
            }
            SelectObject(mem_dc, old_font);
            let _ = DeleteObject(HGDIOBJ(title_font.0));
        }

        let body_top = title_top + title_h + s(3.0);
        if w > pad_x * 2 && body_top + 12 <= body_bottom_limit {
            let body_font = make_font(s(12.5).max(11), FW_NORMAL.0 as i32);
            let old_font = SelectObject(mem_dc, HGDIOBJ(body_font.0));
            let _ = SetTextColor(mem_dc, colorref(palette.secondary));
            let mut body_rc = RECT {
                left: pad_x,
                top: body_top,
                right: w - pad_x,
                bottom: body_bottom_limit,
            };
            let mut body_utf16: Vec<u16> = state.body.encode_utf16().collect();
            if !body_utf16.is_empty() {
                DrawTextW(
                    mem_dc,
                    &mut body_utf16,
                    &mut body_rc,
                    DT_CENTER | DT_WORDBREAK | DT_END_ELLIPSIS | DT_NOPREFIX,
                );
            }
            SelectObject(mem_dc, old_font);
            let _ = DeleteObject(HGDIOBJ(body_font.0));
        }

        // Countdown progress bar ("barra de tiempo") at bottom of card.
        if state.config.show_progress_bar {
            let track_l = s(PROGRESS_SIDE_INSET);
            let track_r = (w - s(PROGRESS_SIDE_INSET)).max(track_l + 8);
            let track_h = s(PROGRESS_HEIGHT).max(4);
            let track_b = h - s(PROGRESS_BOTTOM_INSET);
            let track_t = track_b - track_h;
            let track_r_px = track_h;

            let track_color = blend_color(bg_color, palette.outline, 0.55);
            let track_brush = CreateSolidBrush(colorref(track_color));
            let old_brush = SelectObject(mem_dc, HGDIOBJ(track_brush.0));
            let old_pen = SelectObject(mem_dc, GetStockObject(NULL_PEN));
            let _ = RoundRect(
                mem_dc,
                track_l,
                track_t,
                track_r,
                track_b,
                track_r_px,
                track_r_px,
            );

            let track_w = track_r - track_l;
            let fill_w = ((track_w as f32) * progress).round() as i32;
            if fill_w > 1 {
                let fill_color = if state.hovered {
                    blend_color(palette.accent, palette.text, 0.25)
                } else {
                    palette.accent
                };
                let fill_brush = CreateSolidBrush(colorref(fill_color));
                SelectObject(mem_dc, HGDIOBJ(fill_brush.0));
                let fill_r = (track_l + fill_w).min(track_r);
                let _ = RoundRect(
                    mem_dc,
                    track_l,
                    track_t,
                    fill_r,
                    track_b,
                    track_r_px,
                    track_r_px,
                );
                let _ = DeleteObject(HGDIOBJ(fill_brush.0));
            }
            SelectObject(mem_dc, old_pen);
            SelectObject(mem_dc, old_brush);
            let _ = DeleteObject(HGDIOBJ(track_brush.0));
        }

        // Subtle animated accent border glow during entry.
        let glow = if shimmer_t < 1.0 {
            (1.0 - shimmer_t).powi(2)
        } else {
            0.0
        };
        if glow > 0.04 {
            let border_color = blend_color(palette.outline, palette.accent, glow * 0.85);
            let pen = CreatePen(PS_SOLID, s(1.5).max(1), colorref(border_color));
            let old_pen = SelectObject(mem_dc, HGDIOBJ(pen.0));
            let old_brush = SelectObject(mem_dc, GetStockObject(NULL_BRUSH));
            let radius = s(CORNER_RADIUS) * 2;
            let _ = RoundRect(mem_dc, 0, 0, w - 1, h - 1, radius, radius);
            SelectObject(mem_dc, old_brush);
            SelectObject(mem_dc, old_pen);
            let _ = DeleteObject(HGDIOBJ(pen.0));
        }

        let _ = BitBlt(hdc, 0, 0, w, h, Some(mem_dc), 0, 0, SRCCOPY);

        SelectObject(mem_dc, old_bmp);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem_dc);
        let _ = EndPaint(hwnd, &ps);
    }
}

unsafe fn make_font(height_px: i32, weight: i32) -> HFONT {
    let face = wide("Segoe UI");
    // SAFETY: `face` is NUL-terminated.
    unsafe {
        CreateFontW(
            -height_px,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            FF_DONTCARE.0 as u32,
            PCWSTR(face.as_ptr()),
        )
    }
}

unsafe fn blit_rgba_over_bg(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: u32,
    rgba: &[u8],
    bg: egui::Color32,
) {
    let expected = (size as usize) * (size as usize) * 4;
    if rgba.len() != expected || size == 0 {
        return;
    }
    let mut bgra = Vec::with_capacity(expected);
    for px in rgba.chunks_exact(4) {
        let a = u16::from(px[3]);
        let inv = 255 - a;
        let r = ((u16::from(px[0]) * a + u16::from(bg.r()) * inv + 127) / 255) as u8;
        let g = ((u16::from(px[1]) * a + u16::from(bg.g()) * inv + 127) / 255) as u8;
        let b = ((u16::from(px[2]) * a + u16::from(bg.b()) * inv + 127) / 255) as u8;
        bgra.extend_from_slice(&[b, g, r, 255]);
    }

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size as i32,
            biHeight: -(size as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY: `bgra` has `size * size * 4` bytes matching `bmiHeader`.
    unsafe {
        SetDIBitsToDevice(
            hdc,
            x,
            y,
            size,
            size,
            0,
            0,
            0,
            size,
            bgra.as_ptr().cast(),
            &bmi,
            DIB_RGB_COLORS,
        );
    }
}

/// Decodes `picture` (or Supp's app emblem) into an antialiased `px × px` RGBA8 circular image
/// with a subtle `palette.accent` halo ring around the perimeter.
pub(crate) fn rasterize_ringed_avatar(
    picture: Option<&Path>,
    _title: &str,
    px: u32,
    palette: Palette,
) -> Vec<u8> {
    let size = px.max(8);
    let mut raw = vec![0u8; (size as usize) * (size as usize) * 4];
    let ring = palette.accent;
    let ring_thickness = (size as f32 * 0.055).clamp(1.5, 3.0);
    let inner_inset = (ring_thickness + 2.0).ceil() as u32;
    let inner_size = size.saturating_sub(inner_inset * 2).max(4);

    let inner_rgba = if let Some(path) = picture
        && let Ok(img) = image::open(path)
    {
        let (w, h) = (img.width(), img.height());
        if w > 0 && h > 0 {
            let side = w.min(h);
            let cropped = img.crop_imm((w - side) / 2, (h - side) / 2, side, side);
            cropped
                .resize_exact(inner_size, inner_size, image::imageops::FilterType::Triangle)
                .to_rgba8()
                .into_raw()
        } else {
            crate::util::app_icon_rgba(inner_size as usize)
        }
    } else {
        crate::util::app_icon_rgba(inner_size as usize)
    };

    // Draw outer accent halo ring using coverage-antialiased SDF.
    let center = (size as f32) * 0.5;
    let outer_r = center - 0.5;
    let ring_inner_r = outer_r - ring_thickness;
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 + 0.5) - center;
            let dy = (y as f32 + 0.5) - center;
            let dist = (dx * dx + dy * dy).sqrt();
            let outer_cov = (outer_r + 0.5 - dist).clamp(0.0, 1.0);
            let inner_hole = (dist - (ring_inner_r - 0.5)).clamp(0.0, 1.0);
            let ring_cov = (outer_cov * inner_hole).clamp(0.0, 1.0);
            let idx = ((y * size + x) * 4) as usize;
            if ring_cov > 0.0 {
                raw[idx] = ring.r();
                raw[idx + 1] = ring.g();
                raw[idx + 2] = ring.b();
                raw[idx + 3] = (ring_cov * 255.0).round() as u8;
            }
        }
    }

    // Composite inner circular avatar / emblem centered inside the ring.
    let inner_center = (inner_size as f32) * 0.5;
    let inner_r = inner_center - 0.5;
    for iy in 0..inner_size {
        for ix in 0..inner_size {
            let dx = (ix as f32 + 0.5) - inner_center;
            let dy = (iy as f32 + 0.5) - inner_center;
            let dist = (dx * dx + dy * dy).sqrt();
            let cov = (inner_r + 0.5 - dist).clamp(0.0, 1.0);
            if cov <= 0.0 {
                continue;
            }
            let src_idx = ((iy * inner_size + ix) * 4) as usize;
            let src_a = (f32::from(inner_rgba[src_idx + 3]) / 255.0) * cov;
            if src_a <= 0.0 {
                continue;
            }
            let dst_x = inner_inset + ix;
            let dst_y = inner_inset + iy;
            if dst_x < size && dst_y < size {
                let dst_idx = ((dst_y * size + dst_x) * 4) as usize;
                let bg = palette.surface;
                let a = (src_a * 255.0).round() as u16;
                let inv = 255 - a;
                raw[dst_idx] =
                    ((u16::from(inner_rgba[src_idx]) * a + u16::from(bg.r()) * inv + 127) / 255)
                        as u8;
                raw[dst_idx + 1] =
                    ((u16::from(inner_rgba[src_idx + 1]) * a + u16::from(bg.g()) * inv + 127)
                        / 255) as u8;
                raw[dst_idx + 2] =
                    ((u16::from(inner_rgba[src_idx + 2]) * a + u16::from(bg.b()) * inv + 127)
                        / 255) as u8;
                raw[dst_idx + 3] = (cov * 255.0).round() as u8;
            }
        }
    }

    raw
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_rect_supports_all_four_corners_and_sizes() {
        let work = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1032,
        };
        let (nw, nh) = (272, 232);

        let br0 = stack_rect(
            work,
            0,
            1.0,
            NotificationCorner::BottomRight,
            NotificationSize::Normal,
        );
        assert_eq!(br0.right, 1920 - 16);
        assert_eq!(br0.left, br0.right - nw);
        assert_eq!(br0.bottom, 1032 - 16);
        assert_eq!(br0.top, br0.bottom - nh);

        let br1 = stack_rect(
            work,
            1,
            1.0,
            NotificationCorner::BottomRight,
            NotificationSize::Normal,
        );
        assert_eq!(br1.bottom, br0.top - 10);

        let bl0 = stack_rect(
            work,
            0,
            1.0,
            NotificationCorner::BottomLeft,
            NotificationSize::Compact,
        );
        assert_eq!(bl0.left, 16);
        assert_eq!(bl0.right, 16 + 244);
        assert_eq!(bl0.bottom, 1032 - 16);
        assert_eq!(bl0.top, bl0.bottom - 208);

        let tr0 = stack_rect(
            work,
            0,
            1.0,
            NotificationCorner::TopRight,
            NotificationSize::Large,
        );
        assert_eq!(tr0.top, 16);
        assert_eq!(tr0.right, 1920 - 16);
        assert_eq!(tr0.bottom, 16 + 264);

        let tr1 = stack_rect(
            work,
            1,
            1.0,
            NotificationCorner::TopRight,
            NotificationSize::Large,
        );
        assert_eq!(tr1.top, tr0.bottom + 10);

        let tl0 = stack_rect(
            work,
            0,
            1.5,
            NotificationCorner::TopLeft,
            NotificationSize::Normal,
        );
        assert_eq!(tl0.left, 24);
        assert_eq!(tl0.top, 24);
        assert_eq!(tl0.right - tl0.left, 408);
        assert_eq!(tl0.bottom - tl0.top, 348);
    }

    #[test]
    fn entry_effects_converge_to_base_rect_and_max_alpha() {
        let base = RECT {
            left: 1632,
            top: 784,
            right: 1904,
            bottom: 1016,
        };
        for corner in NotificationCorner::ALL {
            for effect in NotificationEffect::ALL {
                let start = compute_entry_frame(base, corner, effect, 1.0, 240, 0.0);
                let end = compute_entry_frame(base, corner, effect, 1.0, 240, 1.0);
                assert_eq!(end.rect.left, base.left, "{corner:?} {effect:?}");
                assert_eq!(end.rect.top, base.top, "{corner:?} {effect:?}");
                assert_eq!(end.rect.right, base.right, "{corner:?} {effect:?}");
                assert_eq!(end.rect.bottom, base.bottom, "{corner:?} {effect:?}");
                assert_eq!(end.alpha, 240, "{corner:?} {effect:?}");
                assert!(start.alpha < end.alpha, "{corner:?} {effect:?}");
            }
        }
        // Spring exhibits a subtle overshoot mid-animation.
        assert!(ease_effect(NotificationEffect::Spring, 0.65) > 1.0);
        // Bloom grows smoothly from smaller than base at t=0.
        let bloom0 = compute_entry_frame(
            base,
            NotificationCorner::BottomRight,
            NotificationEffect::Bloom,
            1.0,
            255,
            0.0,
        );
        assert!((bloom0.rect.right - bloom0.rect.left) < (base.right - base.left));
    }

    #[test]
    fn remaining_fraction_tracks_countdown() {
        let total = Duration::from_secs(10);
        assert!((remaining_fraction(total, total) - 1.0).abs() < 0.001);
        assert!((remaining_fraction(Duration::from_secs(5), total) - 0.5).abs() < 0.001);
        assert_eq!(remaining_fraction(Duration::ZERO, total), 0.0);
    }

    #[test]
    fn ringed_fallback_avatar_has_transparent_corners_and_opaque_center() {
        let size = 56;
        let rgba = rasterize_ringed_avatar(None, "Supp", size, Palette::dark());
        assert_eq!(rgba.len(), (size as usize) * (size as usize) * 4);
        assert_eq!(rgba[3], 0);
        let center_idx = (((size / 2) * size + (size / 2)) * 4 + 3) as usize;
        assert_eq!(rgba[center_idx], 255);
    }
}
