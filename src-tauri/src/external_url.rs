use std::sync::Mutex;
use std::time::{Duration, Instant};

static LAST_EXTERNAL_OPEN: Mutex<Option<(String, Instant)>> = Mutex::new(None);
const DEDUP_WINDOW: Duration = Duration::from_millis(400);

/// Valida que la URL use únicamente los esquemas permitidos (`http`, `https`, `mailto`).
pub fn validate_external_url(raw: &str) -> Result<tauri::Url, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("URL vacía".to_string());
    }

    let parsed = tauri::Url::parse(trimmed).map_err(|e| format!("URL inválida: {}", e))?;
    match parsed.scheme() {
        "http" | "https" => {
            if parsed.host_str().map(|h| !h.is_empty()).unwrap_or(false) {
                Ok(parsed)
            } else {
                Err("URL HTTP/HTTPS sin host válido".to_string())
            }
        }
        "mailto" => {
            if parsed.path().is_empty() && parsed.query().is_none() {
                Err("URL mailto vacía".to_string())
            } else {
                Ok(parsed)
            }
        }
        other => Err(format!("Esquema de URL no permitido: {}", other)),
    }
}

/// Comprueba de forma estricta si un host pertenece a WhatsApp (`whatsapp.com` o `whatsapp.net`).
pub fn is_whatsapp_host(host: &str) -> bool {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    h == "whatsapp.com"
        || h.ends_with(".whatsapp.com")
        || h == "whatsapp.net"
        || h.ends_with(".whatsapp.net")
}

/// Determina si la navegación principal dentro del WebView2 de sesión debe permitirse.
pub fn should_allow_webview_navigation(url: &tauri::Url) -> bool {
    if url.as_str() == "about:blank" {
        return true;
    }
    match url.scheme() {
        "blob" | "data" => true,
        "http" | "https" => url.host_str().map(is_whatsapp_host).unwrap_or(false),
        _ => false,
    }
}

#[cfg(target_os = "windows")]
mod win_shell {
    use core::ffi::c_void;
    use std::ptr::{null, null_mut};

    const COINIT_APARTMENTTHREADED: u32 = 0x2;
    const COINIT_DISABLE_OLE1DDE: u32 = 0x4;
    const S_OK: i32 = 0;
    const S_FALSE: i32 = 1;
    const SW_SHOWNORMAL: i32 = 1;

    #[link(name = "ole32")]
    extern "system" {
        fn CoInitializeEx(pv_reserved: *mut c_void, dw_co_init: u32) -> i32;
        fn CoUninitialize();
    }

    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteW(
            hwnd: isize,
            lp_operation: *const u16,
            lp_file: *const u16,
            lp_parameters: *const u16,
            lp_directory: *const u16,
            n_show_cmd: i32,
        ) -> isize;
    }

    struct ComGuard(bool);
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    pub fn shell_open_url(url: &str) -> Result<(), String> {
        let wide_op: Vec<u16> = "open\0".encode_utf16().collect();
        let wide_url: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();

        unsafe {
            let hr = CoInitializeEx(null_mut(), COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
            let _com_guard = ComGuard(hr == S_OK || hr == S_FALSE);

            let res = ShellExecuteW(
                0,
                wide_op.as_ptr(),
                wide_url.as_ptr(),
                null(),
                null(),
                SW_SHOWNORMAL,
            );
            if res <= 32 {
                return Err(format!("ShellExecuteW falló con código {}", res));
            }
        }
        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
mod win_shell {
    pub fn shell_open_url(_url: &str) -> Result<(), String> {
        Err("Apertura externa soportada únicamente en Windows".to_string())
    }
}

/// Valida y abre una URL en el navegador predeterminado del sistema (sin consola).
pub fn open_external_url_blocking(raw: &str) -> Result<(), String> {
    let parsed = validate_external_url(raw)?;
    let normalized = parsed.as_str().to_string();

    {
        let mut last = LAST_EXTERNAL_OPEN
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = Instant::now();
        if let Some((ref prev_url, prev_time)) = *last {
            if prev_url == &normalized && now.duration_since(prev_time) < DEDUP_WINDOW {
                return Ok(());
            }
        }
        *last = Some((normalized.clone(), now));
    }

    win_shell::shell_open_url(&normalized)
}

/// Valida síncronamente y lanza la apertura externa en un hilo separado para no bloquear WebView2.
pub fn open_external_url_detached(url: String) {
    if validate_external_url(&url).is_err() {
        return;
    }
    std::thread::spawn(move || {
        let _ = open_external_url_blocking(&url);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_allowed_and_blocked_schemes() {
        assert!(validate_external_url("https://example.com/path?a=1").is_ok());
        assert!(validate_external_url("http://localhost:8080").is_ok());
        assert!(validate_external_url("mailto:user@example.com").is_ok());

        assert!(validate_external_url("javascript:alert(1)").is_err());
        assert!(validate_external_url("file:///C:/Windows/System32/calc.exe").is_err());
        assert!(validate_external_url("data:text/html,hi").is_err());
        assert!(validate_external_url("blob:https://web.whatsapp.com/123").is_err());
        assert!(validate_external_url("https://").is_err());
        assert!(validate_external_url("").is_err());
    }

    #[test]
    fn matches_whatsapp_hosts_strictly() {
        assert!(is_whatsapp_host("web.whatsapp.com"));
        assert!(is_whatsapp_host("whatsapp.com"));
        assert!(is_whatsapp_host("mmg.whatsapp.net"));
        assert!(is_whatsapp_host("WEB.WHATSAPP.COM."));

        assert!(!is_whatsapp_host("evil-whatsapp.com"));
        assert!(!is_whatsapp_host("whatsapp.com.evil.org"));
        assert!(!is_whatsapp_host("wa.me"));
        assert!(!is_whatsapp_host("google.com"));
    }

    #[test]
    fn navigation_policy_blocks_external_hosts() {
        assert!(should_allow_webview_navigation(
            &tauri::Url::parse("https://web.whatsapp.com/").unwrap()
        ));
        assert!(should_allow_webview_navigation(
            &tauri::Url::parse("about:blank").unwrap()
        ));
        assert!(should_allow_webview_navigation(
            &tauri::Url::parse("blob:https://web.whatsapp.com/abc").unwrap()
        ));
        assert!(should_allow_webview_navigation(
            &tauri::Url::parse("data:image/png;base64,AA==").unwrap()
        ));
        assert!(!should_allow_webview_navigation(
            &tauri::Url::parse("https://example.com").unwrap()
        ));
        assert!(!should_allow_webview_navigation(
            &tauri::Url::parse("mailto:test@example.com").unwrap()
        ));
        assert!(!should_allow_webview_navigation(
            &tauri::Url::parse("file:///C:/secret.txt").unwrap()
        ));
    }
}
