//! Starting ZapFast in the tray when the person logs in, through
//! fastframe-autostart.
//!
//! The platform's own login-item record is the only state: Settings reads it
//! back rather than keeping a copy that could disagree with it. Every read and
//! change may block (`SMAppService`, the registry, the Flatpak portal, which can
//! ask the person first), so each runs on a thread of its own.

use std::sync::mpsc::{Receiver, TryRecvError};

pub use fastframe_autostart::{Autostart, Start, State};

fn autostart() -> Autostart {
    Autostart::new("supp", "Supp")
}

/// What a finished read or change found.
#[derive(Debug)]
pub struct Outcome {
    /// The login item as the platform has it now, or why it could not be
    /// changed.
    pub state: Result<State, String>,
    /// Whether the person asked for the change, rather than a read at start.
    pub changed: bool,
}

/// The one read or change of the login item in flight.
#[derive(Default)]
pub struct LoginItem {
    job: Option<Receiver<Outcome>>,
}

impl LoginItem {
    /// Reads the login item, after moving an older macOS registration over
    /// or repairing a legacy Windows `--minimized` Run entry.
    pub fn read(&mut self, wake: impl Fn() + Send + 'static) {
        self.spawn(
            move || {
                #[cfg(target_os = "macos")]
                move_launch_agent();
                #[cfg(windows)]
                repair_legacy_windows_run();
                Outcome {
                    state: Ok(autostart().state()),
                    changed: false,
                }
            },
            wake,
        );
    }

    /// Starts Supp at login (with a window or in the tray), or not at all.
    pub fn set(&mut self, start: Option<Start>, wake: impl Fn() + Send + 'static) {
        self.spawn(
            move || Outcome {
                state: autostart()
                    .set(start)
                    .map_err(|error| error.to_string()),
                changed: true,
            },
            wake,
        );
    }

    fn spawn(
        &mut self,
        work: impl FnOnce() -> Outcome + Send + 'static,
        wake: impl Fn() + Send + 'static,
    ) {
        let (sender, receiver) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("login-item".into())
            .spawn(move || {
                let _ = sender.send(work());
                wake();
            });
        match spawned {
            Ok(_) => self.job = Some(receiver),
            Err(error) => log::warn!("could not read the login item: {error}"),
        }
    }

    /// The finished read or change, if there is one.
    pub fn poll(&mut self) -> Option<Outcome> {
        match self.job.as_ref()?.try_recv() {
            Ok(outcome) => {
                self.job = None;
                Some(outcome)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.job = None;
                None
            }
        }
    }
}

/// Whether Settings offers the switch, and where it stands: `None` where this
/// installation cannot start at login.
pub fn switch(state: State) -> Option<bool> {
    state_parts(state).map(|(enabled, _)| enabled)
}

/// Breaks [`State`] into `(enabled, minimized_in_tray)`, or `None` when
/// unsupported on this installation.
pub fn state_parts(state: State) -> Option<(bool, bool)> {
    match state {
        State::Unsupported => None,
        State::Off => Some((false, false)),
        State::On(Start::Window) | State::NeedsApproval(Start::Window) => Some((true, false)),
        State::On(Start::Hidden) | State::NeedsApproval(Start::Hidden) => Some((true, true)),
    }
}

/// Whether a stored Windows `Run\Supp` command should be rewritten to the
/// current installed `supp.exe` without the legacy `--minimized` flag.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn needs_windows_run_repair(command: &str, current_exe: &str) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return false;
    }
    let current_lower = current_exe.to_ascii_lowercase();
    let installed_binary =
        current_lower.ends_with("supp.exe") && !current_lower.contains(r"\target\");
    if !installed_binary {
        return false;
    }
    let has_legacy_minimized = trimmed
        .split_whitespace()
        .any(|arg| arg.trim_matches('"').eq_ignore_ascii_case("--minimized"));
    if has_legacy_minimized {
        return true;
    }
    // Unquoted executable or pointing to a stale path that no longer exists.
    if !trimmed.starts_with('"') {
        return true;
    }
    if let Some((exe, _)) = trimmed[1..].split_once('"') {
        let exe_clean = exe.strip_prefix(r"\\?\").unwrap_or(exe);
        let cur_clean = current_exe.strip_prefix(r"\\?\").unwrap_or(current_exe);
        if !exe_clean.eq_ignore_ascii_case(cur_clean) {
            return true;
        }
    }
    false
}

#[cfg(windows)]
fn repair_legacy_windows_run() {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW,
    };

    if matches!(autostart().state(), State::Unsupported | State::Off) {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let exe_str = exe.to_string_lossy();
    let key: Vec<u16> = r"Software\Microsoft\Windows\CurrentVersion\Run"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let value: Vec<u16> = "Supp".encode_utf16().chain(std::iter::once(0)).collect();
    let mut size = 0u32;
    // SAFETY: NUL-terminated UTF-16 strings; null buffer queries required size.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if status != ERROR_SUCCESS || size == 0 {
        return;
    }
    let mut bytes = vec![0u8; size as usize];
    // SAFETY: `bytes` is allocated with `size` bytes and valid for the call.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            bytes.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != ERROR_SUCCESS {
        return;
    }
    bytes.truncate(size as usize);
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|&u| u != 0)
        .collect();
    let command = String::from_utf16_lossy(&units);
    if needs_windows_run_repair(&command, &exe_str) {
        let _ = autostart().set(Some(Start::Window));
    }
}

/// ZapFast 0.19 and earlier wrote their own LaunchAgent. Registers the bundled
/// agent in its place and deletes the old file, once; a copy that cannot
/// register (one run outside its bundle) leaves the file as it is.
#[cfg(target_os = "macos")]
fn move_launch_agent() {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let old = std::path::Path::new(&home).join("Library/LaunchAgents/me.paolino.zapfast.plist");
    if !old.is_file() {
        return;
    }
    match autostart().set(Some(Start::Hidden)) {
        Ok(_) => {
            if let Err(error) = std::fs::remove_file(&old) {
                log::warn!("could not remove the old login item: {error}");
            }
        }
        Err(error) => log::info!("the old login item stays: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch_shows_only_where_zapfast_can_start_at_login() {
        assert_eq!(switch(State::Unsupported), None);
        assert_eq!(switch(State::Off), Some(false));
        assert_eq!(switch(State::On(Start::Hidden)), Some(true));
        assert_eq!(switch(State::NeedsApproval(Start::Hidden)), Some(true));
        assert_eq!(switch(State::On(Start::Window)), Some(true));

        assert_eq!(state_parts(State::Unsupported), None);
        assert_eq!(state_parts(State::Off), Some((false, false)));
        assert_eq!(state_parts(State::On(Start::Window)), Some((true, false)));
        assert_eq!(state_parts(State::On(Start::Hidden)), Some((true, true)));
        assert_eq!(
            state_parts(State::NeedsApproval(Start::Window)),
            Some((true, false))
        );
        assert_eq!(
            state_parts(State::NeedsApproval(Start::Hidden)),
            Some((true, true))
        );
    }

    #[test]
    fn detects_legacy_or_unquoted_windows_run_commands() {
        let installed = r"C:\Users\Javier\AppData\Local\Supp\supp.exe";
        assert!(needs_windows_run_repair(
            r"C:\Users\Javier\AppData\Local\Supp\supp.exe --minimized",
            installed,
        ));
        assert!(needs_windows_run_repair(
            r#""C:\Users\Javier\AppData\Local\Supp\supp.exe" --minimized"#,
            installed,
        ));
        assert!(needs_windows_run_repair(
            r"C:\Users\Javier\AppData\Local\Supp\supp.exe",
            installed,
        ));
        assert!(needs_windows_run_repair(
            r#""C:\OldFolder\supp.exe""#,
            installed,
        ));
        assert!(!needs_windows_run_repair(
            r#""C:\Users\Javier\AppData\Local\Supp\supp.exe""#,
            installed,
        ));
        assert!(!needs_windows_run_repair(
            r#""C:\Users\Javier\AppData\Local\Supp\supp.exe" --start-hidden"#,
            installed,
        ));
        // Dev/test builds under \target\ must never touch the user's Run key.
        assert!(!needs_windows_run_repair(
            r"C:\Users\Javier\AppData\Local\Supp\supp.exe --minimized",
            r"C:\repo\target\debug\deps\supp.exe",
        ));
    }
}
