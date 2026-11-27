use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};

const GITHUB_LATEST_API: &str = "https://api.github.com/repos/ixyren/Supp/releases/latest";

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub release_notes: String,
    pub release_url: String,
    pub published_at: String,
    pub asset_name: String,
    pub asset_size: u64,
    pub download_url: String,
    pub is_portable: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgressPayload {
    pub downloaded: u64,
    pub total: u64,
    pub percent: u8,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    size: u64,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    body: Option<String>,
    html_url: String,
    published_at: Option<String>,
    assets: Vec<GitHubAsset>,
}

pub fn parse_semver(raw: &str) -> Option<(u64, u64, u64)> {
    let clean = raw
        .trim()
        .trim_start_matches(|c| c == 'v' || c == 'V')
        .split(&['-', '+'][..])
        .next()?
        .trim();
    if clean.is_empty() {
        return None;
    }
    let mut parts = clean.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

pub fn is_newer_version(current: &str, latest: &str) -> bool {
    match (parse_semver(current), parse_semver(latest)) {
        (Some(cur), Some(lat)) => lat > cur,
        _ => false,
    }
}

pub fn is_allowed_update_url(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 2048 {
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
    h == "github.com"
        || h.ends_with(".github.com")
        || h == "githubusercontent.com"
        || h.ends_with(".githubusercontent.com")
}

pub fn sanitize_asset_name(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.len() > 128
        || trimmed.contains("..")
        || !trimmed
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
        || !trimmed.to_ascii_lowercase().ends_with(".exe")
    {
        return Err("Nombre de instalador o ejecutable inválido".to_string());
    }
    Ok(trimmed.to_string())
}

pub fn detect_is_portable() -> bool {
    let Ok(exe_path) = std::env::current_exe() else {
        return true;
    };
    let Some(dir) = exe_path.parent() else {
        return true;
    };
    if dir.join("uninstall.exe").exists() {
        return false;
    }
    let dir_lower = dir.to_string_lossy().to_ascii_lowercase();
    if dir_lower.contains("\\appdata\\local\\supp")
        || dir_lower.contains("\\program files\\supp")
        || dir_lower.contains("\\program files (x86)\\supp")
    {
        return false;
    }
    true
}

#[cfg(target_os = "windows")]
mod wininet {
    use core::ffi::c_void;
    use std::fs::File;
    use std::io::Write;
    use std::ptr::null;

    const INTERNET_OPEN_TYPE_PRECONFIG: u32 = 0;
    const INTERNET_FLAG_RELOAD: u32 = 0x8000_0000;
    const INTERNET_FLAG_NO_CACHE_WRITE: u32 = 0x0400_0000;
    const INTERNET_FLAG_SECURE: u32 = 0x0080_0000;
    const HTTP_QUERY_CONTENT_LENGTH: u32 = 5;
    const HTTP_QUERY_STATUS_CODE: u32 = 19;
    const HTTP_QUERY_FLAG_NUMBER: u32 = 0x2000_0000;

    #[link(name = "wininet")]
    extern "system" {
        fn InternetOpenW(
            lpsz_agent: *const u16,
            dw_access_type: u32,
            lpsz_proxy: *const u16,
            lpsz_proxy_bypass: *const u16,
            dw_flags: u32,
        ) -> isize;
        fn InternetOpenUrlW(
            h_internet: isize,
            lpsz_url: *const u16,
            lpsz_headers: *const u16,
            dw_headers_length: u32,
            dw_flags: u32,
            dw_context: usize,
        ) -> isize;
        fn HttpQueryInfoW(
            h_request: isize,
            dw_info_level: u32,
            lp_buffer: *mut c_void,
            lpdw_buffer_length: *mut u32,
            lpdw_index: *mut u32,
        ) -> i32;
        fn InternetReadFile(
            h_file: isize,
            lp_buffer: *mut u8,
            dw_number_of_bytes_to_read: u32,
            lpdw_number_of_bytes_read: *mut u32,
        ) -> i32;
        fn InternetCloseHandle(h_internet: isize) -> i32;
    }

    struct InternetHandle(isize);
    impl Drop for InternetHandle {
        fn drop(&mut self) {
            if self.0 != 0 {
                unsafe {
                    InternetCloseHandle(self.0);
                }
            }
        }
    }

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn open_url(url: &str, headers: &str) -> Result<(InternetHandle, InternetHandle, u32, u64), String> {
        let agent_w = to_wide("Supp-Updater/1.0");
        let h_session = unsafe {
            InternetOpenW(
                agent_w.as_ptr(),
                INTERNET_OPEN_TYPE_PRECONFIG,
                null(),
                null(),
                0,
            )
        };
        if h_session == 0 {
            return Err("No se pudo inicializar WinINet para buscar actualizaciones".to_string());
        }
        let session_guard = InternetHandle(h_session);

        let url_w = to_wide(url);
        let headers_w = to_wide(headers);
        let flags = INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE | INTERNET_FLAG_SECURE;
        let h_req = unsafe {
            InternetOpenUrlW(
                session_guard.0,
                url_w.as_ptr(),
                headers_w.as_ptr(),
                u32::MAX,
                flags,
                0,
            )
        };
        if h_req == 0 {
            return Err("No se pudo conectar con GitHub. Verifica tu conexión a internet.".to_string());
        }
        let req_guard = InternetHandle(h_req);

        let mut status: u32 = 0;
        let mut status_len: u32 = std::mem::size_of::<u32>() as u32;
        let mut idx: u32 = 0;
        let got_status = unsafe {
            HttpQueryInfoW(
                req_guard.0,
                HTTP_QUERY_STATUS_CODE | HTTP_QUERY_FLAG_NUMBER,
                &mut status as *mut u32 as *mut c_void,
                &mut status_len,
                &mut idx,
            )
        };
        if got_status != 0 && status >= 400 {
            return Err(format!("GitHub devolvió un error HTTP {}", status));
        }

        let mut content_len: u32 = 0;
        let mut cl_len: u32 = std::mem::size_of::<u32>() as u32;
        let mut cl_idx: u32 = 0;
        let got_cl = unsafe {
            HttpQueryInfoW(
                req_guard.0,
                HTTP_QUERY_CONTENT_LENGTH | HTTP_QUERY_FLAG_NUMBER,
                &mut content_len as *mut u32 as *mut c_void,
                &mut cl_len,
                &mut cl_idx,
            )
        };
        let total_size = if got_cl != 0 { content_len as u64 } else { 0 };

        Ok((session_guard, req_guard, status, total_size))
    }

    pub fn get_text(url: &str, headers: &str) -> Result<String, String> {
        let (_session, req, _status, _total) = open_url(url, headers)?;
        let mut out = Vec::with_capacity(16 * 1024);
        let mut buf = [0u8; 16 * 1024];

        loop {
            let mut bytes_read: u32 = 0;
            let ok = unsafe {
                InternetReadFile(
                    req.0,
                    buf.as_mut_ptr(),
                    buf.len() as u32,
                    &mut bytes_read,
                )
            };
            if ok == 0 {
                return Err("Error leyendo respuesta de GitHub".to_string());
            }
            if bytes_read == 0 {
                break;
            }
            out.extend_from_slice(&buf[..bytes_read as usize]);
        }

        String::from_utf8(out).map_err(|_| "Respuesta inválida de GitHub".to_string())
    }

    pub fn download_to_file<F>(
        url: &str,
        file: &mut File,
        mut on_progress: F,
    ) -> Result<u64, String>
    where
        F: FnMut(u64, u64),
    {
        let (_session, req, _status, total_size) =
            open_url(url, "Accept: application/octet-stream\r\n")?;
        let mut buf = [0u8; 32 * 1024];
        let mut downloaded: u64 = 0;
        let mut last_reported: u64 = 0;

        on_progress(0, total_size);

        loop {
            let mut bytes_read: u32 = 0;
            let ok = unsafe {
                InternetReadFile(
                    req.0,
                    buf.as_mut_ptr(),
                    buf.len() as u32,
                    &mut bytes_read,
                )
            };
            if ok == 0 {
                return Err("Error descargando actualización desde GitHub".to_string());
            }
            if bytes_read == 0 {
                break;
            }
            file.write_all(&buf[..bytes_read as usize])
                .map_err(|e| format!("Error guardando actualización: {}", e))?;
            downloaded += bytes_read as u64;

            if downloaded.saturating_sub(last_reported) >= 64 * 1024 {
                last_reported = downloaded;
                on_progress(downloaded, total_size);
            }
        }

        file.flush()
            .map_err(|e| format!("Error finalizando archivo descargado: {}", e))?;
        on_progress(downloaded, total_size.max(downloaded));
        Ok(downloaded)
    }
}

#[cfg(not(target_os = "windows"))]
mod wininet {
    use std::fs::File;

    pub fn get_text(_url: &str, _headers: &str) -> Result<String, String> {
        Err("Autoactualización soportada únicamente en Windows".to_string())
    }

    pub fn download_to_file<F>(
        _url: &str,
        _file: &mut File,
        _on_progress: F,
    ) -> Result<u64, String>
    where
        F: FnMut(u64, u64),
    {
        Err("Autoactualización soportada únicamente en Windows".to_string())
    }
}

fn select_release_asset(assets: &[GitHubAsset], is_portable: bool) -> Option<&GitHubAsset> {
    let find_setup = || {
        assets.iter().find(|a| {
            let lower = a.name.to_ascii_lowercase();
            lower.ends_with("setup.exe") && is_allowed_update_url(&a.browser_download_url)
        })
    };
    let find_portable = || {
        assets.iter().find(|a| {
            let lower = a.name.to_ascii_lowercase();
            lower.contains("portable")
                && lower.ends_with(".exe")
                && is_allowed_update_url(&a.browser_download_url)
        })
    };

    if is_portable {
        find_portable().or_else(find_setup)
    } else {
        find_setup().or_else(find_portable)
    }
}

pub fn check_for_updates_blocking() -> Result<UpdateInfo, String> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let json_text = wininet::get_text(
        GITHUB_LATEST_API,
        "Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\n",
    )?;

    let release: GitHubRelease = serde_json::from_str(&json_text)
        .map_err(|e| format!("No se pudo leer la información del release: {}", e))?;

    let latest_version = release
        .tag_name
        .trim()
        .trim_start_matches(|c| c == 'v' || c == 'V')
        .to_string();
    let is_portable = detect_is_portable();
    let has_update = is_newer_version(&current_version, &latest_version);

    let chosen_asset = select_release_asset(&release.assets, is_portable);
    let (asset_name, asset_size, download_url) = match chosen_asset {
        Some(a) => (
            a.name.clone(),
            a.size,
            a.browser_download_url.clone(),
        ),
        None => (String::new(), 0, String::new()),
    };

    Ok(UpdateInfo {
        current_version,
        latest_version,
        has_update,
        release_notes: release.body.unwrap_or_default(),
        release_url: release.html_url,
        published_at: release.published_at.unwrap_or_default(),
        asset_name,
        asset_size,
        download_url,
        is_portable,
    })
}

fn prepare_updates_dir() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("supp_updates");
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| format!("Error creando carpeta temporal: {}", e))?;
    } else if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let _ = fs::remove_file(path);
            }
        }
    }
    Ok(dir)
}

#[cfg(target_os = "windows")]
fn launch_downloaded_update(dest_path: &PathBuf, is_portable: bool) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    if !is_portable {
        std::process::Command::new(dest_path)
            .spawn()
            .map_err(|e| format!("No se pudo iniciar el instalador: {}", e))?;
        return Ok(());
    }

    let current_exe = std::env::current_exe()
        .map_err(|e| format!("No se pudo localizar el ejecutable actual: {}", e))?;
    let src_str = dest_path.to_string_lossy().to_string();
    let dst_str = current_exe.to_string_lossy().to_string();
    if src_str.contains('"') || dst_str.contains('"') {
        return Err("Ruta con comillas no soportada para actualización portable".to_string());
    }

    let script_path = dest_path
        .parent()
        .ok_or("Directorio temporal inválido")?
        .join("apply_portable_update.cmd");

    let script_body = format!(
        "@echo off\r\n\
         set \"SRC={src}\"\r\n\
         set \"DST={dst}\"\r\n\
         for /L %%i in (1,1,20) do (\r\n\
           ping -n 2 127.0.0.1 >nul\r\n\
           copy /Y \"%SRC%\" \"%DST%\" >nul 2>&1 && goto :relaunch\r\n\
         )\r\n\
         goto :cleanup\r\n\
         :relaunch\r\n\
         del /Q \"%SRC%\" >nul 2>&1\r\n\
         start \"\" \"%DST%\"\r\n\
         :cleanup\r\n\
         (goto) 2>nul & del \"%~f0\"\r\n",
        src = src_str,
        dst = dst_str
    );

    fs::write(&script_path, script_body)
        .map_err(|e| format!("No se pudo escribir el script de actualización: {}", e))?;

    std::process::Command::new("cmd")
        .args(["/C", &script_path.to_string_lossy()])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("No se pudo ejecutar la actualización portable: {}", e))?;

    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn launch_downloaded_update(_dest_path: &PathBuf, _is_portable: bool) -> Result<(), String> {
    Err("Instalación automática soportada únicamente en Windows".to_string())
}

pub fn download_and_install_update_blocking(
    app: &AppHandle,
    download_url: &str,
    asset_name: &str,
    is_portable: bool,
) -> Result<(), String> {
    if !is_allowed_update_url(download_url) {
        return Err("URL de actualización no permitida".to_string());
    }
    let clean_name = sanitize_asset_name(asset_name)?;
    let updates_dir = prepare_updates_dir()?;
    let dest_path = updates_dir.join(clean_name);

    {
        let mut file = File::create(&dest_path)
            .map_err(|e| format!("No se pudo crear archivo de actualización: {}", e))?;
        let app_clone = app.clone();
        let written = wininet::download_to_file(download_url, &mut file, |downloaded, total| {
            let percent = if total > 0 {
                ((downloaded.saturating_mul(100)) / total).min(100) as u8
            } else {
                0
            };
            let _ = app_clone.emit(
                "update-download-progress",
                UpdateProgressPayload {
                    downloaded,
                    total,
                    percent,
                },
            );
        })?;

        if written == 0 {
            return Err("El archivo descargado está vacío".to_string());
        }
    }

    launch_downloaded_update(&dest_path, is_portable)?;
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_semver_versions() {
        assert_eq!(parse_semver("1.0.5"), Some((1, 0, 5)));
        assert_eq!(parse_semver("v1.2.10"), Some((1, 2, 10)));
        assert!(is_newer_version("1.0.4", "1.0.5"));
        assert!(is_newer_version("1.0.9", "1.1.0"));
        assert!(is_newer_version("1.2.0", "2.0.0"));
        assert!(!is_newer_version("1.0.5", "1.0.5"));
        assert!(!is_newer_version("1.0.6", "1.0.5"));
        assert!(!is_newer_version("bad", "1.0.5"));
    }

    #[test]
    fn validates_github_update_urls() {
        assert!(is_allowed_update_url(
            "https://github.com/ixyren/Supp/releases/download/v1.0.5/Supp_1.0.5_x64-setup.exe"
        ));
        assert!(is_allowed_update_url(
            "https://objects.githubusercontent.com/github-production-release-asset/12345"
        ));

        assert!(!is_allowed_update_url("http://github.com/ixyren/Supp"));
        assert!(!is_allowed_update_url("https://evil-github.com/malware.exe"));
        assert!(!is_allowed_update_url("file:///C:/setup.exe"));
        assert!(!is_allowed_update_url(""));
    }

    #[test]
    fn validates_asset_names() {
        assert_eq!(
            sanitize_asset_name("Supp_1.0.5_x64-setup.exe").unwrap(),
            "Supp_1.0.5_x64-setup.exe"
        );
        assert!(sanitize_asset_name("../evil.exe").is_err());
        assert!(sanitize_asset_name("bad name.exe").is_err());
        assert!(sanitize_asset_name("Supp.msi").is_err());
    }
}
