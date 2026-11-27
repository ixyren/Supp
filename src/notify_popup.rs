//! Floating Telegram-style desktop notification popups on Windows.
//!
//! Renders bottom-right toast cards styled with Supp's active [`Palette`].
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
    CLIP_DEFAULT_PRECIS, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW,
    CreateRoundRectRgn, CreateSolidBrush, DEFAULT_CHARSET, DIB_RGB_COLORS, DT_CENTER,
    DT_END_ELLIPSIS, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, DeleteDC, DeleteObject,
    DrawTextW, EndPaint, FF_DONTCARE, FW_NORMAL, FW_SEMIBOLD, FillRect, GetStockObject, HFONT,
    HGDIOBJ, InvalidateRect, NULL_PEN, OUT_DEFAULT_PRECIS, PAINTSTRUCT, RoundRect, SRCCOPY,
    ScreenToClient, SelectObject, SetBkMode, SetDIBitsToDevice, SetTextColor, SetWindowRgn,
    TRANSPARENT,
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
    SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetCursor,
    SetLayeredWindowAttributes, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    SystemParametersInfoW, TranslateMessage, WM_APP, WM_DESTROY, WM_ERASEBKGND, WM_LBUTTONUP,
    WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_NCDESTROY, WM_PAINT, WM_RBUTTONUP, WM_TIMER, WNDCLASSEXW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{BOOL, PCWSTR};

use crate::notify::NotificationTarget;
use crate::theme::Palette;

/// Logical card dimensions and layout constants (scaled per monitor DPI).
const CARD_WIDTH: f32 = 360.0;
const CARD_HEIGHT: f32 = 86.0;
const MARGIN_RIGHT: f32 = 16.0;
const MARGIN_BOTTOM: f32 = 16.0;
const STACK_GAP: f32 = 10.0;
const CORNER_RADIUS: f32 = 12.0;
const AVATAR_SIZE: f32 = 44.0;
const AVATAR_X: f32 = 16.0;
const CLOSE_SIZE: f32 = 24.0;
const CLOSE_RIGHT_INSET: f32 = 8.0;
const CLOSE_TOP_INSET: f32 = 8.0;
const MAX_VISIBLE: usize = 4;

const ENTER_DURATION: Duration = Duration::from_millis(180);
const LEAVE_DURATION: Duration = Duration::from_millis(150);
const SLIDE_OFFSET_DP: f32 = 14.0;
const MAX_ALPHA: u8 = 248;
const RESUME_MIN_REMAINING: Duration = Duration::from_millis(1500);

const TIMER_TICK: usize = 1;
const TICK_MS: u32 = 16;
const WM_APP_CMD: u32 = WM_APP + 1;
const WM_MOUSELEAVE: u32 = 0x02A3;

const SUPP_LOGO_PNG: &[u8] = include_bytes!("../packaging/windows/supp.png");

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Returns a unique popup identifier for cancellation and tracking.
pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Request to show a floating popup notification on Windows.
pub struct PopupRequest {
    pub id: u64,
    pub title: String,
    pub body: String,
    pub picture: Option<PathBuf>,
    pub palette: Palette,
    pub target: NotificationTarget,
    pub opened: Arc<Mutex<Vec<NotificationTarget>>>,
    pub wake: Arc<dyn Fn() + Send + Sync>,
    pub duration: Duration,
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

/// Shows a Telegram-style popup card on the bottom-right corner of the primary work area.
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
}

thread_local! {
    static HOST: RefCell<PopupHost> = RefCell::new(PopupHost::default());
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
    avatar_rgba: Vec<u8>,
    avatar_px: u32,
    palette: Palette,
    target: NotificationTarget,
    opened: Arc<Mutex<Vec<NotificationTarget>>>,
    wake: Arc<dyn Fn() + Send + Sync>,
    duration: Duration,
    dpi_scale: f32,
    width_px: i32,
    base_rect: RECT,
    phase: Phase,
    hovered: bool,
    close_hovered: bool,
    tracking_leave: bool,
    current_alpha: u8,
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

/// Computes the physical target rectangle for a toast at `slot` (0 = bottom-most),
/// sitting cleanly inside `work_area` above the Windows taskbar.
pub(crate) fn stack_rect(work_area: RECT, slot: usize, dpi_scale: f32) -> RECT {
    let w = (CARD_WIDTH * dpi_scale).round() as i32;
    let h = (CARD_HEIGHT * dpi_scale).round() as i32;
    let mr = (MARGIN_RIGHT * dpi_scale).round() as i32;
    let mb = (MARGIN_BOTTOM * dpi_scale).round() as i32;
    let gap = (STACK_GAP * dpi_scale).round() as i32;

    let right = work_area.right - mr;
    let left = right - w;
    let bottom = work_area.bottom - mb - (slot as i32) * (h + gap);
    let top = bottom - h;
    RECT {
        left,
        top,
        right,
        bottom,
    }
}

fn close_rect_px(width_px: i32, dpi_scale: f32) -> RECT {
    let size = (CLOSE_SIZE * dpi_scale).round() as i32;
    let right = width_px - (CLOSE_RIGHT_INSET * dpi_scale).round() as i32;
    let top = (CLOSE_TOP_INSET * dpi_scale).round() as i32;
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

    let work_area = primary_work_area();
    let init_rect = stack_rect(work_area, 0, 1.0);
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

    let base_rect = stack_rect(work_area, 0, dpi_scale);
    let width_px = base_rect.right - base_rect.left;
    let height_px = base_rect.bottom - base_rect.top;
    let slide_px = (SLIDE_OFFSET_DP * dpi_scale).round() as i32;

    let avatar_px = (AVATAR_SIZE * dpi_scale).round().max(16.0) as u32;
    let avatar_rgba = rasterize_circular_avatar(req.picture.as_deref(), avatar_px, req.palette);

    let now = Instant::now();
    let state = Box::new(ToastState {
        id: req.id,
        title: req.title,
        body: req.body,
        avatar_rgba,
        avatar_px,
        palette: req.palette,
        target: req.target,
        opened: req.opened,
        wake: req.wake,
        duration: req.duration,
        dpi_scale,
        width_px,
        base_rect,
        phase: Phase::Entering { started: now },
        hovered: false,
        close_hovered: false,
        tracking_leave: false,
        current_alpha: 0,
    });
    let state_ptr = Box::into_raw(state);

    // SAFETY: `hwnd` is valid and belongs to this thread; `state_ptr` is freed on `WM_NCDESTROY`.
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
        apply_window_chrome(hwnd, width_px, height_px, dpi_scale, &req.palette);
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_ALPHA);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            base_rect.left,
            base_rect.top + slide_px,
            width_px,
            height_px,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = InvalidateRect(Some(hwnd), None, false);
        SetTimer(Some(hwnd), TIMER_TICK, TICK_MS, None);
    }

    // Insert newest toast at slot 0 (bottom-right) and shift older ones upward.
    // Evict any toast beyond `MAX_VISIBLE`.
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
) {
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
        if rounded.is_err() {
            let radius = (CORNER_RADIUS * dpi_scale).round() as i32;
            let rgn = CreateRoundRectRgn(0, 0, width_px + 1, height_px + 1, radius * 2, radius * 2);
            if !rgn.is_invalid() && SetWindowRgn(hwnd, Some(rgn), true) == 0 {
                let _ = DeleteObject(HGDIOBJ(rgn.0));
            }
        }
    }
}

fn restack_toasts() {
    let work_area = primary_work_area();
    let handles: Vec<isize> = HOST.with(|host| host.borrow().toasts.clone());
    for (slot, raw_hwnd) in handles.into_iter().enumerate() {
        let hwnd = HWND(raw_hwnd as *mut c_void);
        // SAFETY: `hwnd` lives on this thread; `GWLP_USERDATA` is `ToastState`.
        unsafe {
            let Some(state) = toast_state_mut(hwnd) else {
                continue;
            };
            let new_rect = stack_rect(work_area, slot, state.dpi_scale);
            if new_rect.left != state.base_rect.left || new_rect.top != state.base_rect.top {
                state.base_rect = new_rect;
                let slide_y = current_slide_offset(state, Instant::now());
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    new_rect.left,
                    new_rect.top + slide_y,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOSIZE,
                );
            }
        }
    }
}

fn current_slide_offset(state: &ToastState, now: Instant) -> i32 {
    match state.phase {
        Phase::Entering { started } => {
            let t = (now.saturating_duration_since(started).as_secs_f32()
                / ENTER_DURATION.as_secs_f32())
            .clamp(0.0, 1.0);
            let inv = 1.0 - t;
            let eased = 1.0 - inv * inv * inv;
            ((1.0 - eased) * SLIDE_OFFSET_DP * state.dpi_scale).round() as i32
        }
        Phase::Visible { .. } | Phase::Leaving { .. } => 0,
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
    match state.phase {
        Phase::Entering { started } => {
            let elapsed = now.saturating_duration_since(started);
            let t = (elapsed.as_secs_f32() / ENTER_DURATION.as_secs_f32()).clamp(0.0, 1.0);
            let inv = 1.0 - t;
            let eased = 1.0 - inv * inv * inv;
            let alpha = (eased * f32::from(MAX_ALPHA)).round() as u8;
            let slide_y = ((1.0 - eased) * SLIDE_OFFSET_DP * state.dpi_scale).round() as i32;
            // SAFETY: `hwnd` is a live layered window on this thread.
            unsafe {
                if alpha != state.current_alpha {
                    state.current_alpha = alpha;
                    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
                }
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    state.base_rect.left,
                    state.base_rect.top + slide_y,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOSIZE,
                );
            }
            if t >= 1.0 {
                state.phase = Phase::Visible {
                    remaining: state.duration,
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
                }
            }
        }
        Phase::Leaving { started } => {
            let elapsed = now.saturating_duration_since(started);
            let t = (elapsed.as_secs_f32() / LEAVE_DURATION.as_secs_f32()).clamp(0.0, 1.0);
            let alpha = ((1.0 - t) * f32::from(MAX_ALPHA)).round() as u8;
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

        let bg_color = if state.hovered {
            state.palette.surface_hover
        } else {
            state.palette.surface
        };
        let bg_brush = CreateSolidBrush(colorref(bg_color));
        FillRect(mem_dc, &client, bg_brush);
        let _ = DeleteObject(HGDIOBJ(bg_brush.0));

        let scale = state.dpi_scale;
        let s = |dp: f32| (dp * scale).round() as i32;

        // Left accent pill matching Supp's active theme accent color.
        let accent_brush = CreateSolidBrush(colorref(state.palette.accent));
        let old_brush = SelectObject(mem_dc, HGDIOBJ(accent_brush.0));
        let old_pen = SelectObject(mem_dc, GetStockObject(NULL_PEN));
        let pill_w = s(3.0).max(2);
        let pill_r = pill_w.max(2);
        let _ = RoundRect(
            mem_dc,
            s(5.0),
            s(12.0),
            s(5.0) + pill_w,
            h - s(12.0),
            pill_r,
            pill_r,
        );
        SelectObject(mem_dc, old_pen);
        SelectObject(mem_dc, old_brush);
        let _ = DeleteObject(HGDIOBJ(accent_brush.0));

        // Circular avatar / logo composited cleanly against `bg_color`.
        let avatar_x = s(AVATAR_X);
        let avatar_y = (h - state.avatar_px as i32) / 2;
        blit_rgba_over_bg(
            mem_dc,
            avatar_x,
            avatar_y,
            state.avatar_px,
            &state.avatar_rgba,
            bg_color,
        );

        let _ = SetBkMode(mem_dc, TRANSPARENT);

        let text_left = avatar_x + state.avatar_px as i32 + s(12.0);
        let close_rc = close_rect_px(w, scale);
        let text_right = close_rc.left - s(8.0);

        // Title line (chat / sender name).
        if text_right > text_left {
            let title_font = make_font(s(15.0).max(12), FW_SEMIBOLD.0 as i32);
            let old_font = SelectObject(mem_dc, HGDIOBJ(title_font.0));
            let _ = SetTextColor(mem_dc, colorref(state.palette.text));
            let mut title_rc = RECT {
                left: text_left,
                top: s(15.0),
                right: text_right,
                bottom: s(37.0),
            };
            let mut title_utf16: Vec<u16> = state.title.encode_utf16().collect();
            if !title_utf16.is_empty() {
                DrawTextW(
                    mem_dc,
                    &mut title_utf16,
                    &mut title_rc,
                    DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
                );
            }
            SelectObject(mem_dc, old_font);
            let _ = DeleteObject(HGDIOBJ(title_font.0));

            // Body lines (message preview).
            let body_font = make_font(s(13.5).max(11), FW_NORMAL.0 as i32);
            let old_font = SelectObject(mem_dc, HGDIOBJ(body_font.0));
            let _ = SetTextColor(mem_dc, colorref(state.palette.secondary));
            let mut body_rc = RECT {
                left: text_left,
                top: s(38.0),
                right: text_right,
                bottom: h - s(12.0),
            };
            let mut body_utf16: Vec<u16> = state.body.encode_utf16().collect();
            if !body_utf16.is_empty() {
                DrawTextW(
                    mem_dc,
                    &mut body_utf16,
                    &mut body_rc,
                    DT_WORDBREAK | DT_END_ELLIPSIS | DT_NOPREFIX,
                );
            }
            SelectObject(mem_dc, old_font);
            let _ = DeleteObject(HGDIOBJ(body_font.0));
        }

        // Top-right close button (`×`), visible on hover or subtly dimmed.
        if state.close_hovered {
            let hover_brush = CreateSolidBrush(colorref(state.palette.surface_active));
            let old_brush = SelectObject(mem_dc, HGDIOBJ(hover_brush.0));
            let old_pen = SelectObject(mem_dc, GetStockObject(NULL_PEN));
            let _ = RoundRect(
                mem_dc,
                close_rc.left,
                close_rc.top,
                close_rc.right,
                close_rc.bottom,
                s(12.0),
                s(12.0),
            );
            SelectObject(mem_dc, old_pen);
            SelectObject(mem_dc, old_brush);
            let _ = DeleteObject(HGDIOBJ(hover_brush.0));
        }
        let close_font = make_font(s(15.0).max(12), FW_NORMAL.0 as i32);
        let old_font = SelectObject(mem_dc, HGDIOBJ(close_font.0));
        let close_color = if state.close_hovered {
            state.palette.text
        } else {
            state.palette.dim
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

/// Decodes `picture` (or the bundled Supp logo on an accent disc) into an
/// antialiased `px × px` RGBA8 circular image.
pub(crate) fn rasterize_circular_avatar(
    picture: Option<&Path>,
    px: u32,
    palette: Palette,
) -> Vec<u8> {
    let size = px.max(1);
    if let Some(path) = picture
        && let Ok(img) = image::open(path)
    {
        let (w, h) = (img.width(), img.height());
        if w > 0 && h > 0 {
            let side = w.min(h);
            let cropped = img.crop_imm((w - side) / 2, (h - side) / 2, side, side);
            let resized = cropped
                .resize_exact(size, size, image::imageops::FilterType::Triangle)
                .to_rgba8();
            let mut raw = resized.into_raw();
            apply_circle_mask(&mut raw, size);
            return raw;
        }
    }

    // Fallback: Supp's logo centered on a circular `palette.accent` disc.
    let mut raw = vec![0u8; (size as usize) * (size as usize) * 4];
    let bg = palette.accent;
    for chunk in raw.chunks_exact_mut(4) {
        chunk[0] = bg.r();
        chunk[1] = bg.g();
        chunk[2] = bg.b();
        chunk[3] = 255;
    }

    if let Ok(logo) = image::load_from_memory(SUPP_LOGO_PNG) {
        let inset_size = ((size as f32) * 0.64).round().max(1.0) as u32;
        let resized = logo
            .resize(inset_size, inset_size, image::imageops::FilterType::Triangle)
            .to_rgba8();
        let ox = (size - resized.width()) / 2;
        let oy = (size - resized.height()) / 2;
        for ly in 0..resized.height() {
            for lx in 0..resized.width() {
                let src = resized.get_pixel(lx, ly);
                let a = u16::from(src[3]);
                if a == 0 {
                    continue;
                }
                let idx = (((oy + ly) * size + (ox + lx)) * 4) as usize;
                let inv = 255 - a;
                raw[idx] = ((u16::from(src[0]) * a + u16::from(raw[idx]) * inv + 127) / 255) as u8;
                raw[idx + 1] =
                    ((u16::from(src[1]) * a + u16::from(raw[idx + 1]) * inv + 127) / 255) as u8;
                raw[idx + 2] =
                    ((u16::from(src[2]) * a + u16::from(raw[idx + 2]) * inv + 127) / 255) as u8;
            }
        }
    }

    apply_circle_mask(&mut raw, size);
    raw
}

fn apply_circle_mask(rgba: &mut [u8], size: u32) {
    let center = (size as f32) * 0.5;
    let radius = center - 0.5;
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 + 0.5) - center;
            let dy = (y as f32 + 0.5) - center;
            let dist = (dx * dx + dy * dy).sqrt();
            let coverage = (radius + 0.5 - dist).clamp(0.0, 1.0);
            let idx = ((y * size + x) * 4 + 3) as usize;
            rgba[idx] = ((f32::from(rgba[idx]) * coverage).round()) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_rect_places_slot_zero_in_bottom_right_and_stacks_upward() {
        let work = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1032,
        };
        let first = stack_rect(work, 0, 1.0);
        assert_eq!(first.right, 1920 - 16);
        assert_eq!(first.left, first.right - 360);
        assert_eq!(first.bottom, 1032 - 16);
        assert_eq!(first.top, first.bottom - 86);

        let second = stack_rect(work, 1, 1.0);
        assert_eq!(second.right, first.right);
        assert_eq!(second.bottom, first.top - 10);
        assert_eq!(second.top, second.bottom - 86);

        let scaled = stack_rect(work, 0, 1.5);
        assert_eq!(scaled.right - scaled.left, 540);
        assert_eq!(scaled.bottom - scaled.top, 129);
    }

    #[test]
    fn fallback_avatar_is_circular_with_transparent_corners_and_opaque_center() {
        let size = 44;
        let rgba = rasterize_circular_avatar(None, size, Palette::dark());
        assert_eq!(rgba.len(), (size as usize) * (size as usize) * 4);
        // Top-left corner pixel must be transparent.
        assert_eq!(rgba[3], 0);
        // Center pixel must be opaque.
        let center_idx = (((size / 2) * size + (size / 2)) * 4 + 3) as usize;
        assert_eq!(rgba[center_idx], 255);
    }
}
