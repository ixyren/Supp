import React, { useEffect, useRef } from 'react';
import { motion as Motion } from 'framer-motion';
import { MessageSquare } from 'lucide-react';
import { Bridge } from '../bridge';

export function SessionWebview({
  session,
  isActive,
  onReady,
  onUpdateSession,
  onLoadFail,
  animationVariants,
}) {
  const webviewRef = useRef(null);

  useEffect(() => {
    if (!Bridge.isElectron()) {
      onReady(session.id);
      return;
    }

    if (!isActive) return;

    const handleKeyDown = (e) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'r') {
        e.preventDefault();
        if (webviewRef.current) {
          onUpdateSession(session.id, { hasError: false, errorMsg: null });
          webviewRef.current.reload();
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
    };
  }, [isActive, session.id, onReady, onUpdateSession]);

  useEffect(() => {
    if (!Bridge.isElectron()) return;

    const webview = webviewRef.current;
    if (!webview) return;

    const handleDomReady = () => {
      if (!session.isReady) {
        onReady(session.id);
      }
    };

    const handleTitleUpdate = () => {
      try {
        const title = webview.getTitle();
        const match = title.match(/^\((\d+)\)/);
        const unreadCount = match ? parseInt(match[1], 10) : 0;
        onUpdateSession(session.id, {
          unread: unreadCount,
          name:
            unreadCount === 0 && title !== 'WhatsApp' && title !== 'WhatsApp Web'
              ? title
              : session.name,
        });
      } catch (e) {
        console.error(e);
      }
    };

    const handleFailLoad = (e) => {
      if (e.isMainFrame && e.errorCode !== -3) {
        onLoadFail(session.id, e.errorDescription);
      }
    };

    const handleStartLoading = () => {
      onUpdateSession(session.id, { hasError: false, errorMsg: null });
    };

    webview.addEventListener('dom-ready', handleDomReady);
    webview.addEventListener('page-title-updated', handleTitleUpdate);
    webview.addEventListener('did-fail-load', handleFailLoad);
    webview.addEventListener('did-start-loading', handleStartLoading);

    return () => {
      webview.removeEventListener('dom-ready', handleDomReady);
      webview.removeEventListener('page-title-updated', handleTitleUpdate);
      webview.removeEventListener('did-fail-load', handleFailLoad);
      webview.removeEventListener('did-start-loading', handleStartLoading);
    };
  }, [session.id, session.isReady, session.name, onReady, onUpdateSession, onLoadFail]);

  // In Tauri, native WebView2 child views are managed by Rust
  if (Bridge.isTauri()) {
    return null;
  }

  // Electron fallback
  if (Bridge.isElectron()) {
    return (
      <Motion.div
        animate={animationVariants}
        className={`absolute inset-0 w-full h-full transition-opacity duration-300 ${
          isActive ? 'opacity-100' : 'opacity-0'
        }`}
        style={{ pointerEvents: isActive ? 'auto' : 'none' }}
      >
        <webview
          ref={webviewRef}
          src="https://web.whatsapp.com"
          partition={`persist:whatsapp_${session.id}`}
          allowpopups="true"
          webpreferences="contextIsolation=no"
          useragent="Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
          className={`w-full h-full ${session.hasError ? 'hidden' : ''}`}
        />
      </Motion.div>
    );
  }

  // Browser preview fallback
  return (
    <div
      className={`absolute inset-0 w-full h-full flex flex-col items-center justify-center p-8 text-center transition-opacity duration-300 ${
        isActive ? 'opacity-100' : 'opacity-0 pointer-events-none'
      }`}
    >
      <div className="max-w-md bg-[#181c13] border border-[#2c3323] p-8 rounded-[28px] m3-elevation-3 flex flex-col items-center">
        <div className="w-16 h-16 rounded-[22px] bg-[#2a3e00] border border-[#c6ff00]/30 text-[#c6ff00] flex items-center justify-center mb-5 shadow-lg shadow-[#c6ff00]/15">
          <MessageSquare size={30} />
        </div>
        <h3 className="text-xl font-bold text-[#e7ebdc] mb-2">{session.name}</h3>
        <p className="text-xs text-[#a2a895] leading-relaxed mb-4">
          Sesión aislada con motor nativo de Rust (WebView2). Al ejecutar la aplicación compilada con Tauri, WhatsApp Web carga aquí de forma fluida con consumo mínimo de RAM.
        </p>
        <div className="text-[11px] font-mono bg-[#21271a] border border-[#2c3323] px-3 py-1.5 rounded-full text-[#c6ff00]">
          ID: {session.id}
        </div>
      </div>
    </div>
  );
}
