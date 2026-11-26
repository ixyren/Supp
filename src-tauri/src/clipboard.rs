use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(1);
const MAX_IMAGE_DIM: u32 = 16384;
const MAX_MEDIA_BYTES: usize = 128 * 1024 * 1024;
const STALE_TEMP_AGE: Duration = Duration::from_secs(3600);

/// Decodifica una cadena Base64 estándar o URL-safe en bytes sin dependencias externas.
pub fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    let cleaned: Vec<u8> = input
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();

    if cleaned.is_empty() {
        return Ok(Vec::new());
    }
    if cleaned.len() % 4 == 1 {
        return Err("Longitud Base64 inválida".to_string());
    }

    let mut out = Vec::with_capacity((cleaned.len() / 4) * 3 + 3);
    let mut i = 0;

    let val = |b: u8| -> Result<u8, String> {
        match b {
            b'A'..=b'Z' => Ok(b - b'A'),
            b'a'..=b'z' => Ok(b - b'a' + 26),
            b'0'..=b'9' => Ok(b - b'0' + 52),
            b'+' | b'-' => Ok(62),
            b'/' | b'_' => Ok(63),
            _ => Err("Carácter Base64 inválido".to_string()),
        }
    };

    while i + 4 <= cleaned.len() {
        let a = cleaned[i];
        let b = cleaned[i + 1];
        let c = cleaned[i + 2];
        let d = cleaned[i + 3];
        i += 4;

        let v0 = val(a)?;
        let v1 = val(b)?;

        if c == b'=' {
            if d != b'=' || i != cleaned.len() {
                return Err("Padding Base64 inválido".to_string());
            }
            out.push((v0 << 2) | (v1 >> 4));
        } else if d == b'=' {
            if i != cleaned.len() {
                return Err("Padding Base64 inválido".to_string());
            }
            let v2 = val(c)?;
            out.push((v0 << 2) | (v1 >> 4));
            out.push((v1 << 4) | (v2 >> 2));
        } else {
            let v2 = val(c)?;
            let v3 = val(d)?;
            out.push((v0 << 2) | (v1 >> 4));
            out.push((v1 << 4) | (v2 >> 2));
            out.push((v2 << 6) | v3);
        }
    }

    let rem = cleaned.len() - i;
    if rem == 2 {
        let v0 = val(cleaned[i])?;
        let v1 = val(cleaned[i + 1])?;
        out.push((v0 << 2) | (v1 >> 4));
    } else if rem == 3 {
        let v0 = val(cleaned[i])?;
        let v1 = val(cleaned[i + 1])?;
        let v2 = val(cleaned[i + 2])?;
        out.push((v0 << 2) | (v1 >> 4));
        out.push((v1 << 4) | (v2 >> 2));
    }

    Ok(out)
}

/// Construye un bloque `CF_DIB` (`BITMAPINFOHEADER` 32bpp BGRA bottom-up) desde RGBA.
pub fn build_cf_dib_from_rgba(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    if width == 0 || height == 0 || width > MAX_IMAGE_DIM || height > MAX_IMAGE_DIM {
        return Err("Dimensiones de imagen fuera de rango".to_string());
    }

    let pixel_count = (width as usize)
        .checked_mul(height as usize)
        .ok_or("Desbordamiento de dimensiones")?;
    let expected_len = pixel_count
        .checked_mul(4)
        .ok_or("Imagen demasiado grande")?;
    if expected_len > MAX_MEDIA_BYTES {
        return Err("La imagen supera el límite de memoria".to_string());
    }
    if rgba.len() != expected_len {
        return Err(format!(
            "Tamaño RGBA inesperado: {} (esperado {})",
            rgba.len(),
            expected_len
        ));
    }

    let total_len = 40usize
        .checked_add(expected_len)
        .ok_or("Imagen demasiado grande")?;
    let mut dib = Vec::with_capacity(total_len);

    // BITMAPINFOHEADER (40 bytes, biHeight positivo = filas bottom-up compatibles universalmente)
    dib.extend_from_slice(&40u32.to_le_bytes()); // biSize
    dib.extend_from_slice(&(width as i32).to_le_bytes()); // biWidth
    dib.extend_from_slice(&(height as i32).to_le_bytes()); // biHeight
    dib.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    dib.extend_from_slice(&32u16.to_le_bytes()); // biBitCount (BGRA)
    dib.extend_from_slice(&0u32.to_le_bytes()); // biCompression = BI_RGB
    dib.extend_from_slice(&(expected_len as u32).to_le_bytes()); // biSizeImage
    dib.extend_from_slice(&0i32.to_le_bytes()); // biXPelsPerMeter
    dib.extend_from_slice(&0i32.to_le_bytes()); // biYPelsPerMeter
    dib.extend_from_slice(&0u32.to_le_bytes()); // biClrUsed
    dib.extend_from_slice(&0u32.to_le_bytes()); // biClrImportant

    let row_bytes = (width as usize) * 4;
    for y in (0..height as usize).rev() {
        let row = &rgba[y * row_bytes..(y + 1) * row_bytes];
        for px in row.chunks_exact(4) {
            dib.push(px[2]); // B
            dib.push(px[1]); // G
            dib.push(px[0]); // R
            dib.push(px[3]); // A
        }
    }

    Ok(dib)
}

/// Construye la estructura `DROPFILES` (UTF-16 doble-nulo-terminada) para `CF_HDROP`.
pub fn build_cf_hdrop_payload(file_path: &Path) -> Result<Vec<u8>, String> {
    let raw_str = file_path
        .to_str()
        .ok_or("Ruta de archivo temporal no válida en UTF-8")?;
    let clean_str = raw_str.strip_prefix(r"\\?\").unwrap_or(raw_str);
    if clean_str.is_empty() {
        return Err("Ruta de archivo vacía".to_string());
    }

    let utf16: Vec<u16> = clean_str.encode_utf16().collect();
    let mut payload = Vec::with_capacity(20 + (utf16.len() + 2) * 2);

    // DROPFILES (20 bytes exactos en Windows)
    payload.extend_from_slice(&20u32.to_le_bytes()); // pFiles
    payload.extend_from_slice(&0i32.to_le_bytes()); // pt.x
    payload.extend_from_slice(&0i32.to_le_bytes()); // pt.y
    payload.extend_from_slice(&0i32.to_le_bytes()); // fNC
    payload.extend_from_slice(&1i32.to_le_bytes()); // fWide = TRUE

    for unit in utf16 {
        payload.extend_from_slice(&unit.to_le_bytes());
    }
    payload.extend_from_slice(&0u16.to_le_bytes()); // fin del archivo
    payload.extend_from_slice(&0u16.to_le_bytes()); // fin de la lista

    Ok(payload)
}

fn sanitize_video_extension(ext: &str) -> &'static str {
    match ext.trim().trim_start_matches('.').to_ascii_lowercase().as_str() {
        "webm" => "webm",
        "mov" => "mov",
        "m4v" => "m4v",
        _ => "mp4",
    }
}

fn write_temp_video_file(video_bytes: &[u8], extension: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("supp_clipboard");
    fs::create_dir_all(&dir).map_err(|e| format!("No se pudo crear carpeta temporal: {}", e))?;

    // Limpiar archivos temporales antiguos (> 1 hora)
    if let Ok(entries) = fs::read_dir(&dir) {
        let now = SystemTime::now();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                let ts = meta.modified().or_else(|_| meta.created());
                if let Ok(modified) = ts {
                    if let Ok(age) = now.duration_since(modified) {
                        if age > STALE_TEMP_AGE {
                            let _ = fs::remove_file(&path);
                        }
                    }
                }
            }
        }
    }

    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let seq = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let safe_ext = sanitize_video_extension(extension);
    let file_path = dir.join(format!("supp_video_{}_{}.{}", millis, seq, safe_ext));

    fs::write(&file_path, video_bytes)
        .map_err(|e| format!("Error escribiendo archivo de video temporal: {}", e))?;

    Ok(file_path)
}

#[cfg(target_os = "windows")]
mod win_clip {
    use core::ffi::c_void;
    use std::ptr::copy_nonoverlapping;
    use std::time::Duration;

    const GMEM_MOVEABLE: u32 = 0x0002;
    const CF_DIB: u32 = 8;
    const CF_HDROP: u32 = 15;

    #[link(name = "user32")]
    extern "system" {
        fn OpenClipboard(h_wnd_new_owner: isize) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn SetClipboardData(u_format: u32, h_mem: *mut c_void) -> *mut c_void;
        fn RegisterClipboardFormatW(lpsz_format: *const u16) -> u32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalAlloc(u_flags: u32, dw_bytes: usize) -> *mut c_void;
        fn GlobalLock(h_mem: *mut c_void) -> *mut c_void;
        fn GlobalUnlock(h_mem: *mut c_void) -> i32;
        fn GlobalFree(h_mem: *mut c_void) -> *mut c_void;
    }

    struct ClipboardGuard;
    impl Drop for ClipboardGuard {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
            }
        }
    }

    fn open_and_empty_clipboard() -> Result<ClipboardGuard, String> {
        for attempt in 0..8 {
            if unsafe { OpenClipboard(0) } != 0 {
                let guard = ClipboardGuard;
                if unsafe { EmptyClipboard() } == 0 {
                    return Err("EmptyClipboard falló".to_string());
                }
                return Ok(guard);
            }
            if attempt < 7 {
                std::thread::sleep(Duration::from_millis(15));
            }
        }
        Err("No se pudo abrir el portapapeles de Windows".to_string())
    }

    unsafe fn set_clipboard_bytes(format: u32, data: &[u8]) -> Result<(), String> {
        if data.is_empty() {
            return Err("Datos vacíos para el portapapeles".to_string());
        }
        let hmem = GlobalAlloc(GMEM_MOVEABLE, data.len());
        if hmem.is_null() {
            return Err("GlobalAlloc falló al asignar memoria del portapapeles".to_string());
        }

        let locked = GlobalLock(hmem) as *mut u8;
        if locked.is_null() {
            GlobalFree(hmem);
            return Err("GlobalLock falló".to_string());
        }

        copy_nonoverlapping(data.as_ptr(), locked, data.len());
        GlobalUnlock(hmem);

        let handle = SetClipboardData(format, hmem);
        if handle.is_null() {
            GlobalFree(hmem);
            return Err(format!("SetClipboardData falló para formato {}", format));
        }
        // En éxito, el sistema operativo toma posesión de hmem; no llamar GlobalFree.
        Ok(())
    }

    pub fn write_image(dib: &[u8], png: Option<&[u8]>) -> Result<(), String> {
        let _guard = open_and_empty_clipboard()?;
        unsafe {
            set_clipboard_bytes(CF_DIB, dib)?;
            if let Some(png_bytes) = png {
                if !png_bytes.is_empty() {
                    let png_name: Vec<u16> = "PNG\0".encode_utf16().collect();
                    let png_fmt = RegisterClipboardFormatW(png_name.as_ptr());
                    if png_fmt != 0 {
                        let _ = set_clipboard_bytes(png_fmt, png_bytes);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn write_hdrop(hdrop_payload: &[u8]) -> Result<(), String> {
        let _guard = open_and_empty_clipboard()?;
        unsafe { set_clipboard_bytes(CF_HDROP, hdrop_payload) }
    }
}

#[cfg(not(target_os = "windows"))]
mod win_clip {
    pub fn write_image(_dib: &[u8], _png: Option<&[u8]>) -> Result<(), String> {
        Err("Portapapeles nativo soportado únicamente en Windows".to_string())
    }

    pub fn write_hdrop(_hdrop_payload: &[u8]) -> Result<(), String> {
        Err("Portapapeles nativo soportado únicamente en Windows".to_string())
    }
}

/// Decodifica la imagen RGBA (y PNG opcional) y la escribe en el portapapeles de Windows.
pub fn copy_image_blocking(
    width: u32,
    height: u32,
    rgba_base64: &str,
    png_base64: Option<&str>,
) -> Result<(), String> {
    let rgba = decode_base64(rgba_base64)?;
    let dib = build_cf_dib_from_rgba(width, height, &rgba)?;
    let png = match png_base64 {
        Some(s) if !s.trim().is_empty() => Some(decode_base64(s)?),
        _ => None,
    };
    win_clip::write_image(&dib, png.as_deref())
}

/// Guarda el video temporal en `%TEMP%/supp_clipboard/` y coloca `CF_HDROP` en el portapapeles.
pub fn copy_video_blocking(data_base64: &str, extension: &str) -> Result<(), String> {
    let bytes = decode_base64(data_base64)?;
    if bytes.is_empty() {
        return Err("El archivo de video está vacío".to_string());
    }
    if bytes.len() > MAX_MEDIA_BYTES {
        return Err("El video supera el tamaño máximo soportado (128 MB)".to_string());
    }

    let file_path = write_temp_video_file(&bytes, extension)?;
    let hdrop = build_cf_hdrop_payload(&file_path)?;
    win_clip::write_hdrop(&hdrop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_base64_variants() {
        assert_eq!(decode_base64("").unwrap(), b"");
        assert_eq!(decode_base64("SGVsbG8=").unwrap(), b"Hello");
        assert_eq!(decode_base64("SGVs\nbG8h").unwrap(), b"Hello!");
        assert_eq!(decode_base64("YWI=").unwrap(), b"ab");
        assert_eq!(decode_base64("YQ==").unwrap(), b"a");
        assert!(decode_base64("A").is_err());
        assert!(decode_base64("SGVsb=8=").is_err());
        assert!(decode_base64("SGVs$G8=").is_err());
    }

    #[test]
    fn builds_bottom_up_bgra_cf_dib() {
        // 2x2 RGBA:
        // fila 0 (arriba): rojo (255,0,0,255), verde (0,255,0,255)
        // fila 1 (abajo): azul (0,0,255,128), blanco (255,255,255,255)
        let rgba = vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 128, 255, 255, 255, 255,
        ];
        let dib = build_cf_dib_from_rgba(2, 2, &rgba).unwrap();
        assert_eq!(dib.len(), 40 + 16);
        assert_eq!(&dib[0..4], &40u32.to_le_bytes());
        assert_eq!(&dib[4..8], &2i32.to_le_bytes());
        assert_eq!(&dib[8..12], &2i32.to_le_bytes());
        assert_eq!(&dib[14..16], &32u16.to_le_bytes());

        // Primera fila en DIB debe ser fila 1 (abajo) en BGRA
        assert_eq!(&dib[40..48], &[255, 0, 0, 128, 255, 255, 255, 255]);
        // Segunda fila en DIB debe ser fila 0 (arriba) en BGRA
        assert_eq!(&dib[48..56], &[0, 0, 255, 255, 0, 255, 0, 255]);
    }

    #[test]
    fn builds_utf16_double_null_cf_hdrop() {
        let p = Path::new(r"\\?\C:\Temp\a.mp4");
        let payload = build_cf_hdrop_payload(p).unwrap();
        assert_eq!(&payload[0..4], &20u32.to_le_bytes()); // pFiles
        assert_eq!(&payload[16..20], &1i32.to_le_bytes()); // fWide = 1

        let expected_utf16: Vec<u16> = r"C:\Temp\a.mp4"
            .encode_utf16()
            .chain([0, 0])
            .collect();
        assert_eq!(payload.len(), 20 + expected_utf16.len() * 2);
        for (idx, chunk) in payload[20..].chunks_exact(2).enumerate() {
            let code = u16::from_le_bytes([chunk[0], chunk[1]]);
            assert_eq!(code, expected_utf16[idx]);
        }
    }
}
