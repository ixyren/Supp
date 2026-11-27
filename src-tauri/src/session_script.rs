/// Genera el script de inicialización inyectado en cada WebView2 de sesión de WhatsApp Web.
pub fn build_init_script(session_id: &str) -> String {
    let session_id_json = serde_json::to_string(session_id).unwrap_or_else(|_| "\"\"".to_string());

    format!(
        r#"
(function() {{
    if (window !== window.top) return;

    const SESSION_ID = {session_id_json};
    window.__SUPP_SESSION_ID__ = SESSION_ID;

    const invokeCmd = (cmd, args) => {{
        try {{
            if (window.__TAURI_INTERNALS__ && typeof window.__TAURI_INTERNALS__.invoke === 'function') {{
                return window.__TAURI_INTERNALS__.invoke(cmd, args).catch(() => {{}});
            }}
        }} catch (_) {{}}
        return Promise.resolve();
    }};

    const invokeCmdStrict = (cmd, args) => {{
        if (!window.__TAURI_INTERNALS__ || typeof window.__TAURI_INTERNALS__.invoke !== 'function') {{
            return Promise.reject(new Error('IPC de Tauri no disponible'));
        }}
        return window.__TAURI_INTERNALS__.invoke(cmd, args);
    }};

    const isWhatsappHost = (host) => {{
        const h = String(host || '').replace(/\.$/, '').toLowerCase();
        return (
            h === 'whatsapp.com' ||
            h.endsWith('.whatsapp.com') ||
            h === 'whatsapp.net' ||
            h.endsWith('.whatsapp.net')
        );
    }};

    // ── Contador de mensajes no leídos ─────────────────────────────────────────
    let lastTitle = "";
    const checkTitle = () => {{
        const title = document.title || "";
        if (title !== lastTitle) {{
            lastTitle = title;
            const match = title.match(/^\((\d+)\)/);
            const unread = match ? parseInt(match[1], 10) : 0;
            invokeCmd('report_unread', {{ sessionId: SESSION_ID, unread: unread }});
        }}
    }};
    setInterval(checkTitle, 1000);

    // ── Detección de carga real de WhatsApp Web (lista de chats o QR) ──────────
    let readyReported = false;
    let stableReadyTicks = 0;
    const notifyReady = () => {{
        if (readyReported) return;
        readyReported = true;
        invokeCmd('report_session_ready', {{ sessionId: SESSION_ID }});
        if (typeof extractProfileAvatar === 'function') {{
            extractProfileAvatar();
            setTimeout(extractProfileAvatar, 450);
            setTimeout(extractProfileAvatar, 1500);
        }}
    }};

    const checkWhatsAppReady = () => {{
        if (readyReported) return;
        if (!isWhatsappHost(location.hostname)) {{
            if (document.readyState === 'complete') {{
                stableReadyTicks++;
                if (stableReadyTicks >= 2) notifyReady();
            }}
            return;
        }}

        const hasChatUI = Boolean(
            document.querySelector('#pane-side, #side, [data-testid="chat-list"], [data-testid="chatlist-header"]')
        );
        const hasQrUI = Boolean(
            document.querySelector('[data-testid="qrcode"], [data-ref], .landing-main, .landing-wrapper, canvas[aria-label]')
        );
        const hasStartup = Boolean(
            document.getElementById('startup') ||
            document.getElementById('initial_startup') ||
            document.querySelector('progress')
        );

        if ((hasChatUI || hasQrUI) && !hasStartup) {{
            stableReadyTicks++;
            if (stableReadyTicks >= 2) {{
                notifyReady();
            }}
        }} else {{
            stableReadyTicks = 0;
        }}
    }};
    const readyInterval = setInterval(() => {{
        checkWhatsAppReady();
        if (readyReported) clearInterval(readyInterval);
    }}, 180);
    setTimeout(() => {{
        clearInterval(readyInterval);
        notifyReady();
    }}, 6000);

    // ── Notificar clics al sidebar principal (para colapsar hover) ─────────────
    window.addEventListener('pointerdown', function(e) {{
        const menu = document.getElementById('supp-media-context-menu');
        if (menu && menu.contains(e.target)) return;
        invokeCmd('report_webview_click');
    }}, true);

    // ── Toast flotante Dark-Lime ───────────────────────────────────────────────
    window.__suppShowToast = function(msg, type) {{
        try {{
            const attach = () => {{
                const parent = document.body || document.documentElement;
                if (!parent) {{
                    setTimeout(attach, 100);
                    return;
                }}
                if (!document.getElementById('supp-toast-keyframes')) {{
                    const styleTag = document.createElement('style');
                    styleTag.id = 'supp-toast-keyframes';
                    styleTag.textContent = '@keyframes suppSpin {{ from {{ transform: rotate(0deg); }} to {{ transform: rotate(360deg); }} }}';
                    parent.appendChild(styleTag);
                }}
                let el = document.getElementById('supp-floating-toast');
                if (!el) {{
                    el = document.createElement('div');
                    el.id = 'supp-floating-toast';
                    parent.appendChild(el);
                }}
                const isError = type === 'error';
                const isLoading = type === 'loading';
                const borderCol = isError ? 'rgba(239, 68, 68, 0.6)' : 'rgba(198, 255, 0, 0.45)';
                const shadowCol = isError ? 'rgba(239, 68, 68, 0.2)' : 'rgba(198, 255, 0, 0.15)';

                el.style.cssText = 'position:fixed;bottom:24px;right:24px;z-index:2147483646;display:inline-flex;align-items:center;gap:8px;background-color:#1a1e15;color:#e3e6d9;border:1px solid ' + borderCol + ';border-radius:9999px;padding:7px 16px;font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif;font-size:12px;font-weight:500;box-shadow:0 8px 24px rgba(0,0,0,0.5),0 0 14px ' + shadowCol + ';pointer-events:none;will-change:transform,opacity;transition:transform 0.28s cubic-bezier(0.16,1,0.3,1),opacity 0.22s cubic-bezier(0.16,1,0.3,1);transform:translate3d(0,18px,0) scale(0.9);opacity:0;';

                const iconSvg = isLoading
                    ? '<svg style="width:14px;height:14px;color:#c6ff00;animation:suppSpin 1.1s linear infinite;flex-shrink:0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M21 12a9 9 0 1 1-6.219-8.56"/></svg>'
                    : isError
                    ? '<svg style="width:14px;height:14px;color:#ef4444;flex-shrink:0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>'
                    : '<svg style="width:14px;height:14px;color:#c6ff00;flex-shrink:0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>';

                el.innerHTML = iconSvg;
                const labelSpan = document.createElement('span');
                labelSpan.style.cssText = 'white-space:nowrap;letter-spacing:0.01em;';
                labelSpan.textContent = String(msg == null ? '' : msg);
                el.appendChild(labelSpan);

                requestAnimationFrame(() => {{
                    el.style.transform = 'translate3d(0,0,0) scale(1)';
                    el.style.opacity = '1';
                }});
                clearTimeout(window.__suppToastTimer);
                window.__suppToastTimer = setTimeout(() => {{
                    el.style.transform = 'translate3d(0,12px,0) scale(0.94)';
                    el.style.opacity = '0';
                    setTimeout(() => {{ if (el && el.parentNode) el.parentNode.removeChild(el); }}, 280);
                }}, isLoading ? 3500 : 2800);
            }};
            attach();
        }} catch (e) {{ console.error("Toast error:", e); }}
    }};

    // ── Interceptar clics en enlaces <a href> sin duplicar on_new_window ───────
    document.addEventListener('click', function(e) {{
        if (e.defaultPrevented || e.button !== 0) return;
        const target = e.target;
        const anchor = target && target.closest ? target.closest('a[href]') : null;
        if (!anchor || anchor.hasAttribute('download')) return;

        const rawHref = (anchor.getAttribute('href') || '').trim();
        if (!rawHref || rawHref.startsWith('#') || /^javascript:/i.test(rawHref)) return;

        let parsed;
        try {{
            parsed = new URL(anchor.href, location.href);
        }} catch (_) {{
            return;
        }}

        const isExternalHttp = (parsed.protocol === 'http:' || parsed.protocol === 'https:') && !isWhatsappHost(parsed.hostname);
        const isMailto = parsed.protocol === 'mailto:';
        if (isExternalHttp || isMailto) {{
            e.preventDefault();
            e.stopPropagation();
            invokeCmd('open_external_url', {{ url: parsed.href }});
        }}
    }}, false);

    // ── Menú contextual Supp para Imágenes y Videos ────────────────────────────
    const MENU_ID = 'supp-media-context-menu';
    let closeMenuListenersCleanup = null;

    const closeMediaMenu = () => {{
        const existing = document.getElementById(MENU_ID);
        if (existing && existing.parentNode) {{
            existing.parentNode.removeChild(existing);
        }}
        if (closeMenuListenersCleanup) {{
            closeMenuListenersCleanup();
            closeMenuListenersCleanup = null;
        }}
    }};

    const isElementVisible = (el) => {{
        if (!el || !el.isConnected) return false;
        const st = getComputedStyle(el);
        return st.display !== 'none' && st.visibility !== 'hidden' && parseFloat(st.opacity || '1') > 0.05;
    }};

    const isTinyOrEmojiImg = (el, rect) => {{
        if (rect.width < 64 || rect.height < 64) return true;
        if (el.classList && el.classList.contains('emoji')) return true;
        if (el.hasAttribute && el.hasAttribute('data-plain-text')) return true;
        if (el.complete && el.naturalWidth > 0 && (el.naturalWidth < 64 || el.naturalHeight < 64)) return true;
        return false;
    }};

    const findMediaTarget = (e) => {{
        const rawTarget = e.target;
        if (rawTarget && rawTarget.closest) {{
            if (rawTarget.closest('#' + MENU_ID)) return null;
            if (rawTarget.closest('input, textarea, [contenteditable="true"]')) return null;
        }}

        const selection = window.getSelection ? String(window.getSelection()).trim() : '';
        if (selection.length > 0 && rawTarget && rawTarget.tagName !== 'IMG' && rawTarget.tagName !== 'VIDEO') {{
            return null;
        }}

        const stack = typeof document.elementsFromPoint === 'function'
            ? document.elementsFromPoint(e.clientX, e.clientY)
            : (rawTarget ? [rawTarget] : []);

        let fallbackImg = null;

        for (const el of stack) {{
            if (!el || !el.tagName) continue;
            if (el.closest && el.closest('#pane-side')) continue;
            if (!isElementVisible(el)) continue;

            const tag = el.tagName.toUpperCase();
            if (tag === 'VIDEO') {{
                const rect = el.getBoundingClientRect();
                if (rect.width < 64 || rect.height < 64) continue;
                const srcEl = el.querySelector('source');
                const src = el.currentSrc || el.src || (srcEl ? srcEl.src : '') || '';
                const canFrame = el.readyState >= 2 && el.videoWidth > 0 && el.videoHeight > 0;
                if (src || canFrame) {{
                    return {{ kind: 'video', el, src, canFrame }};
                }}
            }} else if (tag === 'IMG' && !fallbackImg) {{
                const rect = el.getBoundingClientRect();
                if (isTinyOrEmojiImg(el, rect)) continue;
                const src = el.currentSrc || el.src || '';
                if (src) {{
                    fallbackImg = {{ kind: 'img', el, src }};
                }}
            }}
        }}

        return fallbackImg;
    }};

    const uint8ToBase64 = (bytes) => {{
        const CHUNK = 0x8000;
        const parts = [];
        for (let i = 0; i < bytes.length; i += CHUNK) {{
            parts.push(String.fromCharCode.apply(null, bytes.subarray(i, i + CHUNK)));
        }}
        return btoa(parts.join(''));
    }};

    const extractCanvasImage = (source, width, height) => {{
        const canvas = document.createElement('canvas');
        canvas.width = width;
        canvas.height = height;
        const ctx = canvas.getContext('2d', {{ willReadFrequently: true }});
        if (!ctx) throw new Error('Contexto 2D no disponible');
        ctx.drawImage(source, 0, 0, width, height);
        const imgData = ctx.getImageData(0, 0, width, height);
        const rgbaBase64 = uint8ToBase64(imgData.data);
        let pngBase64 = null;
        try {{
            const dataUrl = canvas.toDataURL('image/png');
            if (dataUrl && dataUrl.startsWith('data:image/png;base64,')) {{
                pngBase64 = dataUrl.slice(22);
            }}
        }} catch (_) {{}}
        return {{ width, height, rgbaBase64, pngBase64 }};
    }};

    const fetchBlobWithFallback = async (url) => {{
        try {{
            const res = await fetch(url);
            if (res.ok) return await res.blob();
        }} catch (_) {{}}
        const res2 = await fetch(url, {{ mode: 'cors', credentials: 'omit' }});
        if (!res2.ok) throw new Error('HTTP ' + res2.status);
        return await res2.blob();
    }};

    const decodeBlobToCanvasPayload = async (blob) => {{
        if (typeof createImageBitmap === 'function') {{
            try {{
                const bitmap = await createImageBitmap(blob);
                try {{
                    return extractCanvasImage(bitmap, bitmap.width, bitmap.height);
                }} finally {{
                    bitmap.close();
                }}
            }} catch (_) {{}}
        }}

        const objUrl = URL.createObjectURL(blob);
        try {{
            const img = await new Promise((resolve, reject) => {{
                const loader = new Image();
                loader.onload = () => resolve(loader);
                loader.onerror = () => reject(new Error('No se pudo decodificar la imagen'));
                loader.src = objUrl;
            }});
            return extractCanvasImage(img, img.naturalWidth || img.width, img.naturalHeight || img.height);
        }} finally {{
            URL.revokeObjectURL(objUrl);
        }}
    }};

    const copyImageElement = async (target) => {{
        const img = target.el;
        const src = img.currentSrc || img.src || target.src;
        let payload = null;

        if (img.complete && img.naturalWidth > 0 && img.naturalHeight > 0) {{
            try {{
                payload = extractCanvasImage(img, img.naturalWidth, img.naturalHeight);
            }} catch (_) {{
                // Canvas tainted por CORS -> recuperar vía fetch + Blob
                payload = null;
            }}
        }}

        if (!payload) {{
            if (!src) throw new Error('Origen de imagen no disponible');
            const blob = await fetchBlobWithFallback(src);
            payload = await decodeBlobToCanvasPayload(blob);
        }}

        await invokeCmdStrict('copy_image_to_clipboard', payload);
        window.__suppShowToast('Imagen copiada al portapapeles', 'success');
    }};

    const copyVideoFrame = async (target) => {{
        const video = target.el;
        if (!video || video.videoWidth <= 0 || video.videoHeight <= 0) {{
            throw new Error('Fotograma no disponible');
        }}
        const payload = extractCanvasImage(video, video.videoWidth, video.videoHeight);
        await invokeCmdStrict('copy_image_to_clipboard', payload);
        window.__suppShowToast('Fotograma copiado al portapapeles', 'success');
    }};

    const copyVideoFile = async (target) => {{
        const video = target.el;
        const srcEl = video ? video.querySelector('source') : null;
        const src = (video && (video.currentSrc || video.src)) || (srcEl && srcEl.src) || target.src;
        if (!src) throw new Error('Origen de video no disponible');

        window.__suppShowToast('Copiando video...', 'loading');

        const res = await fetch(src);
        if (!res.ok) throw new Error('HTTP ' + res.status);
        const contentType = (res.headers.get('content-type') || '').toLowerCase();
        const buffer = await res.arrayBuffer();
        if (!buffer || buffer.byteLength === 0) throw new Error('Video vacío');

        let extension = 'mp4';
        if (contentType.includes('webm') || /\.webm(\?|$)/i.test(src)) {{
            extension = 'webm';
        }} else if (contentType.includes('quicktime') || /\.mov(\?|$)/i.test(src)) {{
            extension = 'mov';
        }}

        const dataBase64 = uint8ToBase64(new Uint8Array(buffer));
        await invokeCmdStrict('copy_video_to_clipboard', {{ dataBase64, extension }});
        window.__suppShowToast('Video copiado al portapapeles', 'success');
    }};

    const copyTextUrl = async (url) => {{
        let copied = false;
        if (navigator.clipboard && typeof navigator.clipboard.writeText === 'function') {{
            try {{
                await navigator.clipboard.writeText(url);
                copied = true;
            }} catch (_) {{}}
        }}
        if (!copied) {{
            const ta = document.createElement('textarea');
            ta.value = url;
            ta.style.cssText = 'position:fixed;left:-9999px;top:-9999px;opacity:0;';
            document.body.appendChild(ta);
            ta.select();
            copied = document.execCommand('copy');
            document.body.removeChild(ta);
        }}
        if (!copied) throw new Error('No se pudo copiar el enlace');
        window.__suppShowToast('Enlace copiado al portapapeles', 'success');
    }};

    const createMenuButton = (label, svgMarkup, onClick) => {{
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.style.cssText = 'width:100%;display:flex;align-items:center;gap:9px;padding:8px 10px;border:none;border-radius:10px;background:transparent;color:#e3e6d9;font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif;font-size:12px;font-weight:500;text-align:left;cursor:pointer;outline:none;transition:background-color 120ms ease,color 120ms ease;';

        const iconWrap = document.createElement('span');
        iconWrap.style.cssText = 'width:15px;height:15px;display:inline-flex;align-items:center;justify-content:center;color:#a2a895;flex-shrink:0;transition:color 120ms ease;';
        iconWrap.innerHTML = svgMarkup;

        const textSpan = document.createElement('span');
        textSpan.textContent = label;
        textSpan.style.cssText = 'white-space:nowrap;overflow:hidden;text-overflow:ellipsis;';

        btn.appendChild(iconWrap);
        btn.appendChild(textSpan);

        btn.addEventListener('mouseenter', () => {{
            btn.style.backgroundColor = '#252b1e';
            btn.style.color = '#c6ff00';
            iconWrap.style.color = '#c6ff00';
        }});
        btn.addEventListener('mouseleave', () => {{
            btn.style.backgroundColor = 'transparent';
            btn.style.color = '#e3e6d9';
            iconWrap.style.color = '#a2a895';
        }});
        btn.addEventListener('click', (ev) => {{
            ev.preventDefault();
            ev.stopPropagation();
            closeMediaMenu();
            onClick();
        }});
        return btn;
    }};

    const ICON_COPY = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>';
    const ICON_FRAME = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="3"/><circle cx="8.5" cy="8.5" r="1.5"/><path d="M21 15l-5-5L5 21"/></svg>';
    const ICON_VIDEO = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polygon points="23 7 16 12 23 17 23 7"/><rect x="1" y="5" width="15" height="14" rx="2"/></svg>';
    const ICON_EXTERNAL = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/><polyline points="15 3 21 3 21 9"/><line x1="10" y1="14" x2="21" y2="3"/></svg>';
    const ICON_LINK = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>';

    const showMediaMenu = (e, target) => {{
        closeMediaMenu();

        const parent = document.body || document.documentElement;
        if (!parent) return;

        const menu = document.createElement('div');
        menu.id = MENU_ID;
        menu.style.cssText = 'position:fixed;left:0;top:0;min-width:198px;max-width:250px;background-color:#1a1e15;border:1px solid rgba(198,255,0,0.38);border-radius:14px;padding:6px;z-index:2147483647;box-shadow:0 14px 36px rgba(0,0,0,0.75),0 0 16px rgba(198,255,0,0.14);user-select:none;display:flex;flex-direction:column;gap:2px;will-change:transform,opacity;transform-origin:top left;transform:translate3d(0,-4px,0) scale(0.94);opacity:0;transition:transform 140ms cubic-bezier(0.16,1,0.3,1),opacity 110ms ease;';

        const header = document.createElement('div');
        header.style.cssText = 'display:flex;align-items:center;gap:7px;padding:5px 9px 7px;margin-bottom:3px;border-bottom:1px solid #2f3525;font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif;font-size:11px;font-weight:700;letter-spacing:0.02em;color:#e3e6d9;';
        const dot = document.createElement('span');
        dot.style.cssText = 'width:7px;height:7px;border-radius:9999px;background:#c6ff00;box-shadow:0 0 8px rgba(198,255,0,0.65);flex-shrink:0;';
        const title = document.createElement('span');
        title.textContent = target.kind === 'video' ? 'Opciones de video' : 'Opciones de imagen';
        header.appendChild(dot);
        header.appendChild(title);
        menu.appendChild(header);

        const isHttpUrl = /^https?:\/\//i.test(target.src || '');

        if (target.kind === 'img') {{
            menu.appendChild(createMenuButton('Copiar imagen', ICON_COPY, () => {{
                copyImageElement(target).catch((err) => {{
                    console.error('Copy image error:', err);
                    window.__suppShowToast('No se pudo copiar la imagen', 'error');
                }});
            }}));
        }} else if (target.kind === 'video') {{
            if (target.src) {{
                menu.appendChild(createMenuButton('Copiar video', ICON_VIDEO, () => {{
                    copyVideoFile(target).catch((err) => {{
                        console.error('Copy video error:', err);
                        window.__suppShowToast('No se pudo copiar el video', 'error');
                    }});
                }}));
            }}
            if (target.canFrame) {{
                menu.appendChild(createMenuButton('Copiar fotograma actual', ICON_FRAME, () => {{
                    copyVideoFrame(target).catch((err) => {{
                        console.error('Copy frame error:', err);
                        window.__suppShowToast('No se pudo copiar el fotograma', 'error');
                    }});
                }}));
            }}
        }}

        if (isHttpUrl) {{
            menu.appendChild(createMenuButton('Abrir en navegador', ICON_EXTERNAL, () => {{
                invokeCmdStrict('open_external_url', {{ url: target.src }}).catch(() => {{
                    window.__suppShowToast('No se pudo abrir el enlace', 'error');
                }});
            }}));
            menu.appendChild(createMenuButton('Copiar enlace', ICON_LINK, () => {{
                copyTextUrl(target.src).catch(() => {{
                    window.__suppShowToast('No se pudo copiar el enlace', 'error');
                }});
            }}));
        }}

        menu.addEventListener('contextmenu', (ev) => {{
            ev.preventDefault();
            ev.stopPropagation();
        }});

        parent.appendChild(menu);

        // Ajustar estrictamente dentro del viewport antes de animar la entrada
        const rect = menu.getBoundingClientRect();
        const margin = 8;
        const maxLeft = Math.max(margin, window.innerWidth - rect.width - margin);
        const maxTop = Math.max(margin, window.innerHeight - rect.height - margin);
        const clampedX = Math.min(Math.max(e.clientX, margin), maxLeft);
        const clampedY = Math.min(Math.max(e.clientY, margin), maxTop);
        menu.style.left = clampedX + 'px';
        menu.style.top = clampedY + 'px';

        requestAnimationFrame(() => {{
            menu.style.transform = 'translate3d(0,0,0) scale(1)';
            menu.style.opacity = '1';
        }});

        const onOutsidePointer = (ev) => {{
            if (!menu.contains(ev.target)) {{
                closeMediaMenu();
            }}
        }};
        const onKeyDown = (ev) => {{
            if (ev.key === 'Escape') {{
                closeMediaMenu();
            }}
        }};
        const onClose = () => closeMediaMenu();

        document.addEventListener('pointerdown', onOutsidePointer, true);
        window.addEventListener('keydown', onKeyDown, true);
        window.addEventListener('scroll', onClose, true);
        window.addEventListener('blur', onClose);
        window.addEventListener('resize', onClose);

        closeMenuListenersCleanup = () => {{
            document.removeEventListener('pointerdown', onOutsidePointer, true);
            window.removeEventListener('keydown', onKeyDown, true);
            window.removeEventListener('scroll', onClose, true);
            window.removeEventListener('blur', onClose);
            window.removeEventListener('resize', onClose);
        }};
    }};

    window.addEventListener('contextmenu', function(e) {{
        const target = findMediaTarget(e);
        if (!target) {{
            closeMediaMenu();
            return;
        }}
        e.preventDefault();
        e.stopPropagation();
        showMediaMenu(e, target);
    }}, true);

    // ── Extracción automática de foto de perfil de la cuenta iniciada ─────────
    let lastAvatarSig = '';
    let avatarExtracting = false;

    const safeRequire = (name) => {{
        try {{
            if (typeof window.require === 'function') {{
                return window.require(name);
            }}
        }} catch (_) {{}}
        return null;
    }};

    const unwrapModule = (mod) => (mod && mod.default ? mod.default : mod);

    const pickValidProfilePicUrl = (pic) => {{
        if (!pic) return null;
        const attrs = pic.attributes || pic;
        const candidates = [
            attrs.imgFull,
            attrs.img,
            attrs.eurl,
            pic.imgFull,
            pic.img,
            pic.eurl
        ];
        for (const url of candidates) {{
            if (
                typeof url === 'string' &&
                /^https:\/\//i.test(url) &&
                !/static\.whatsapp\.net/i.test(url)
            ) {{
                return url;
            }}
        }}
        return null;
    }};

    const getAvatarFromWhatsAppStore = async () => {{
        try {{
            const rawPrefs = safeRequire('WAWebUserPrefsMeUser');
            const prefs = unwrapModule(rawPrefs);
            if (!prefs) return null;

            const rawCandidates = [
                typeof prefs.getMaybeMePnUser === 'function' && prefs.getMaybeMePnUser(),
                typeof prefs.getMaybeMeUser === 'function' && prefs.getMaybeMeUser(),
                typeof prefs.getMaybeMeLidUser === 'function' && prefs.getMaybeMeLidUser()
            ].filter(Boolean);
            if (rawCandidates.length === 0) return null;

            const idCandidates = [];
            const seenSerialized = new Set();
            for (const cand of rawCandidates) {{
                idCandidates.push(cand);
                if (cand._serialized && !seenSerialized.has(cand._serialized)) {{
                    seenSerialized.add(cand._serialized);
                    idCandidates.push(cand._serialized);
                }}
            }}

            // 1. Colección directa de miniaturas de perfil
            const rawThumbCol = safeRequire('WAWebProfilePicThumbCollection');
            const thumbCol = unwrapModule(rawThumbCol);
            const colObj = thumbCol && (thumbCol.ProfilePicThumbCollection || thumbCol);
            if (colObj) {{
                for (const id of idCandidates) {{
                    try {{
                        if (typeof colObj.get === 'function') {{
                            const cached = colObj.get(id);
                            const url = pickValidProfilePicUrl(cached);
                            if (url) return url;
                        }}
                    }} catch (_) {{}}
                }}
                for (const id of idCandidates) {{
                    try {{
                        if (typeof colObj.find === 'function') {{
                            const fetched = await Promise.race([
                                colObj.find(id),
                                new Promise((res) => setTimeout(() => res(null), 2200))
                            ]);
                            const url = pickValidProfilePicUrl(fetched);
                            if (url) return url;
                        }}
                    }} catch (_) {{}}
                }}
            }}

            // 2. Bridge moderno de foto de perfil
            const rawBridge = safeRequire('WAWebContactProfilePicThumbBridge');
            const bridge = unwrapModule(rawBridge);
            if (bridge) {{
                for (const fnName of ['profilePicFind', 'requestProfilePicFromServer', 'find']) {{
                    if (typeof bridge[fnName] === 'function') {{
                        for (const id of idCandidates) {{
                            try {{
                                const pic = await Promise.race([
                                    bridge[fnName](id),
                                    new Promise((res) => setTimeout(() => res(null), 2200))
                                ]);
                                const url = pickValidProfilePicUrl(pic);
                                if (url) return url;
                            }} catch (_) {{}}
                        }}
                    }}
                }}
            }}

            // 3. Colección de contactos (propiedad propia del contacto "Me")
            const rawContactCol = safeRequire('WAWebContactCollection');
            const contactCol = unwrapModule(rawContactCol);
            const cObj = contactCol && (contactCol.ContactCollection || contactCol);
            if (cObj && typeof cObj.get === 'function') {{
                for (const id of idCandidates) {{
                    try {{
                        const contact = cObj.get(id);
                        if (contact) {{
                            if (typeof contact.getProfilePicThumb === 'function') {{
                                const pic = contact.getProfilePicThumb();
                                const url = pickValidProfilePicUrl(pic);
                                if (url) return url;
                            }}
                            const url = pickValidProfilePicUrl(contact.profilePicThumb);
                            if (url) return url;
                        }}
                    }} catch (_) {{}}
                }}
            }}
        }} catch (_) {{}}
        return null;
    }};

    const isNonProfileButtonOrSrc = (img, src) => {{
        if (!src) return true;
        if (/static\.whatsapp\.net/i.test(src)) return true;
        if (/(\/|_)meta([_-]?ai)?(\.|\/|_|\?|$)/i.test(src)) return true;
        if (!/^https:\/\/[^/]*pps\.whatsapp\.net\//i.test(src) && !/^blob:/i.test(src)) {{
            return true;
        }}
        const metaOrNavRegex = /\b(meta|ai|ia|canales|channels|comunidades|communities|estados|status|novedades|updates|assistant)\b/i;
        const alt = (img.getAttribute && img.getAttribute('alt')) || '';
        if (metaOrNavRegex.test(alt)) return true;
        let cur = img;
        for (let depth = 0; depth < 6 && cur; depth++) {{
            if (cur.getAttribute) {{
                const aria = cur.getAttribute('aria-label') || '';
                const title = cur.getAttribute('title') || '';
                const testId = cur.getAttribute('data-testid') || '';
                if (metaOrNavRegex.test(aria) || metaOrNavRegex.test(title) || /meta|ai|status|channel|communit/i.test(testId)) {{
                    return true;
                }}
            }}
            cur = cur.parentElement;
        }}
        return false;
    }};

    const findProfileAvatarImg = () => {{
        const isAvatarCandidate = (img) => {{
            if (!img || !img.tagName || img.tagName.toUpperCase() !== 'IMG') return false;
            if (img.closest && (img.closest('#pane-side') || img.closest('#main'))) return false;
            if (!img.complete || img.naturalWidth < 24 || img.naturalHeight < 24) return false;
            if (!document.hidden) {{
                if (!isElementVisible(img)) return false;
                const rect = img.getBoundingClientRect();
                if (rect.width < 20 || rect.height < 20 || rect.width > 96) return false;
            }}
            const src = img.currentSrc || img.src || '';
            if (isNonProfileButtonOrSrc(img, src)) return false;
            return true;
        }};

        const prioritySelectors = [
            '[data-testid="menu-bar-profile"] img',
            '[aria-label*="Perfil" i] img, [aria-label*="Profile" i] img'
        ];

        for (const selector of prioritySelectors) {{
            const nodes = document.querySelectorAll(selector);
            for (const img of nodes) {{
                if (!isAvatarCandidate(img)) continue;
                const src = img.currentSrc || img.src || '';
                return {{ img, src }};
            }}
        }}

        const navImgs = Array.from(
            document.querySelectorAll('header img[src*="pps.whatsapp.net"], header img[src^="blob:"], nav img[src*="pps.whatsapp.net"], nav img[src^="blob:"]')
        ).filter(isAvatarCandidate);

        if (navImgs.length > 0) {{
            // El botón de perfil en el panel lateral moderno está en la parte inferior del rail
            navImgs.sort((a, b) => b.getBoundingClientRect().top - a.getBoundingClientRect().top);
            const chosen = navImgs[0];
            return {{ img: chosen, src: chosen.currentSrc || chosen.src || '' }};
        }}

        return null;
    }};

    const renderAvatarToDataUrl = (source) => {{
        const size = 96;
        const canvas = document.createElement('canvas');
        canvas.width = size;
        canvas.height = size;
        const ctx = canvas.getContext('2d');
        if (!ctx) throw new Error('Contexto 2D no disponible');
        ctx.drawImage(source, 0, 0, size, size);
        try {{
            const jpeg = canvas.toDataURL('image/jpeg', 0.86);
            if (jpeg && jpeg.startsWith('data:image/')) return jpeg;
        }} catch (_) {{}}
        return canvas.toDataURL('image/png');
    }};

    const decodeBlobAvatarDataUrl = async (blob) => {{
        if (typeof createImageBitmap === 'function') {{
            try {{
                const bitmap = await createImageBitmap(blob);
                try {{
                    return renderAvatarToDataUrl(bitmap);
                }} finally {{
                    bitmap.close();
                }}
            }} catch (_) {{}}
        }}

        const objUrl = URL.createObjectURL(blob);
        try {{
            const loader = await new Promise((resolve, reject) => {{
                const img = new Image();
                img.onload = () => resolve(img);
                img.onerror = () => reject(new Error('No se pudo cargar avatar'));
                img.src = objUrl;
            }});
            return renderAvatarToDataUrl(loader);
        }} finally {{
            URL.revokeObjectURL(objUrl);
        }}
    }};

    async function extractProfileAvatar() {{
        if (avatarExtracting) return;
        if (!isWhatsappHost(location.hostname)) return;

        avatarExtracting = true;
        try {{
            // 1. Fuente primaria oficial (WhatsApp Web internal store, igual que whatsapp-web.js / WA-JS)
            const storeUrl = await getAvatarFromWhatsAppStore();
            if (storeUrl) {{
                if (storeUrl === lastAvatarSig) return;
                await invokeCmdStrict('report_session_avatar', {{
                    sessionId: SESSION_ID,
                    avatarDataUrl: storeUrl
                }});
                lastAvatarSig = storeUrl;
                return;
            }}

            // 2. Respaldo estricto por DOM (solo pps.whatsapp.net / blob, excluyendo Meta AI)
            const found = findProfileAvatarImg();
            if (!found) return;

            const {{ img, src }} = found;
            const sig = src + '|' + img.naturalWidth + 'x' + img.naturalHeight;
            if (sig === lastAvatarSig) return;

            let avatarPayload = null;
            try {{
                avatarPayload = renderAvatarToDataUrl(img);
            }} catch (_) {{
                avatarPayload = null;
            }}

            if (!avatarPayload || !avatarPayload.startsWith('data:image/') || avatarPayload.length > 200000) {{
                try {{
                    const blob = await fetchBlobWithFallback(src);
                    avatarPayload = await decodeBlobAvatarDataUrl(blob);
                }} catch (_) {{
                    avatarPayload = null;
                }}
            }}

            if ((!avatarPayload || !avatarPayload.startsWith('data:image/') || avatarPayload.length > 200000) && /^https:\/\//i.test(src)) {{
                avatarPayload = src;
            }}

            if (avatarPayload && ((avatarPayload.startsWith('data:image/') && avatarPayload.length <= 200000) || /^https:\/\//i.test(avatarPayload))) {{
                await invokeCmdStrict('report_session_avatar', {{
                    sessionId: SESSION_ID,
                    avatarDataUrl: avatarPayload
                }});
                lastAvatarSig = sig;
            }}
        }} catch (_) {{
            // Reintentar en el próximo ciclo si la sesión aún se está inicializando
        }} finally {{
            avatarExtracting = false;
        }}
    }}

    window.addEventListener('focus', () => {{
        extractProfileAvatar();
    }});
    document.addEventListener('visibilitychange', () => {{
        if (!document.hidden) {{
            extractProfileAvatar();
            setTimeout(extractProfileAvatar, 600);
        }}
    }});
    setInterval(extractProfileAvatar, 2500);
}})();
"#,
        session_id_json = session_id_json
    )
}
