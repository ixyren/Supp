// Supp Bridge - Universal IPC Bridge (Tauri v2 + Electron + Web Dev Fallback)
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

const invokeRequired = async (command, args) => {
  try {
    return await invoke(command, args);
  } catch (err) {
    console.error(`Error in Tauri ${command}:`, err);
    throw err;
  }
};

export const Bridge = {
  isTauri: () => typeof window !== 'undefined' && Boolean(window.__TAURI_INTERNALS__),
  isElectron: () => typeof window !== 'undefined' && Boolean(window.electron),

  // Get application settings
  getSettings: async () => {
    if (Bridge.isTauri()) {
      try {
        return await invoke('get_settings');
      } catch (e) {
        console.error('Error in Tauri get_settings:', e);
      }
    }
    // Fallback to localStorage / defaults
    return {
      closeToTray: localStorage.getItem('closeToTray') !== 'false',
      startMinimized: localStorage.getItem('startMinimized') === 'true',
      openAtLogin: localStorage.getItem('startWithWindows') === 'true',
    };
  },

  // Set close to tray behavior
  setCloseToTray: async (enable) => {
    localStorage.setItem('closeToTray', enable);
    if (Bridge.isTauri()) {
      return await invokeRequired('set_close_to_tray', { enable });
    } else if (Bridge.isElectron()) {
      return await window.electron.setCloseToTray(enable);
    }
    return { success: true };
  },

  // Set start minimized behavior
  setStartMinimized: async (enable) => {
    localStorage.setItem('startMinimized', enable);
    if (Bridge.isTauri()) {
      return await invokeRequired('set_start_minimized', { enable });
    }
    return { success: true };
  },

  // Toggle startup with Windows
  toggleStartup: async (enable) => {
    localStorage.setItem('startWithWindows', enable);
    if (Bridge.isTauri()) {
      try {
        const { enable: enableAutostart, disable: disableAutostart } = await import('@tauri-apps/plugin-autostart');
        if (enable) {
          await enableAutostart();
        } else {
          await disableAutostart();
        }
        return { success: true };
      } catch (e) {
        console.error('Error in Tauri autostart:', e);
        throw e;
      }
    } else if (Bridge.isElectron()) {
      return await window.electron.toggleStartup(enable);
    }
    return { success: true };
  },

  // Minimize window to tray
  minimizeToTray: async () => {
    if (Bridge.isTauri()) {
      return await invokeRequired('minimize_to_tray');
    } else if (Bridge.isElectron()) {
      return await window.electron.minimizeToTray();
    }
    return { success: true };
  },

  // Quit application completely
  quitApp: async () => {
    if (Bridge.isTauri()) {
      return await invokeRequired('quit_app');
    } else if (Bridge.isElectron()) {
      return await window.electron.quitApp();
    }
    return { success: true };
  },

  // Open an external validated URL (http, https, mailto) in the system default browser
  openExternalUrl: async (url) => {
    if (Bridge.isTauri()) {
      return await invokeRequired('open_external_url', { url });
    }
    if (typeof window !== 'undefined' && /^https?:|^mailto:/i.test(String(url || '').trim())) {
      window.open(url, '_blank', 'noopener,noreferrer');
    }
    return { success: true };
  },

  // Copy RGBA/PNG image payload to the native Windows clipboard
  copyImageToClipboard: async ({ width, height, rgbaBase64, pngBase64 = null }) => {
    if (Bridge.isTauri()) {
      return await invokeRequired('copy_image_to_clipboard', {
        width,
        height,
        rgbaBase64,
        pngBase64,
      });
    }
    return { success: false };
  },

  // Copy video bytes as a temporary file drop (CF_HDROP) to the native Windows clipboard
  copyVideoToClipboard: async ({ dataBase64, extension = 'mp4' }) => {
    if (Bridge.isTauri()) {
      return await invokeRequired('copy_video_to_clipboard', {
        dataBase64,
        extension,
      });
    }
    return { success: false };
  },

  // Multi-session WhatsApp Management in Tauri
  createSessionWebview: async (sessionId, sidebarWidth, isActive = false) => {
    if (Bridge.isTauri()) {
      return await invokeRequired('create_session_webview', {
        sessionId,
        sidebarWidth: sidebarWidth || 80.0,
        isActive: Boolean(isActive),
      });
    }
    return { success: true };
  },

  switchSession: async (activeSessionId, allSessionIds, sidebarWidth, direction, previousSessionId) => {
    if (Bridge.isTauri()) {
      return await invokeRequired('switch_session', {
        activeSessionId: activeSessionId === 'settings' ? null : activeSessionId,
        previousSessionId: previousSessionId === 'settings' ? null : previousSessionId,
        allSessionIds: allSessionIds || [],
        sidebarWidth: sidebarWidth || 80.0,
        direction: direction || 0,
      });
    }
    return { success: true };
  },

  updateSidebarBounds: async (activeSessionId, sidebarWidth) => {
    if (Bridge.isTauri()) {
      try {
        return await invoke('update_sidebar_bounds', {
          activeSessionId: activeSessionId === 'settings' ? null : activeSessionId,
          sidebarWidth: sidebarWidth || 80.0,
        });
      } catch (e) {
        console.error('Error in Tauri update_sidebar_bounds:', e);
        return { success: false, error: e };
      }
    }
    return { success: true };
  },

  finishIntro: async (activeSessionId, allSessionIds, sidebarWidth) => {
    if (Bridge.isTauri()) {
      return await invokeRequired('finish_intro', {
        activeSessionId: activeSessionId === 'settings' ? null : activeSessionId,
        allSessionIds: allSessionIds || [],
        sidebarWidth: sidebarWidth || 80.0,
      });
    }
    return { success: true };
  },

  destroySession: async (sessionId) => {
    if (Bridge.isTauri()) {
      return await invokeRequired('destroy_session', { sessionId });
    }
    return { success: true };
  },

  reloadSession: async (sessionId) => {
    if (Bridge.isTauri()) {
      return await invokeRequired('reload_session', { sessionId });
    }
    return { success: true };
  },

  listenUnread: async (callback) => {
    if (Bridge.isTauri()) {
      try {
        return await listen('session-unread-update', (event) => {
          if (event.payload) {
            callback(event.payload.session_id, event.payload.unread);
          }
        });
      } catch (e) {
        console.warn('Error listening for unread updates in Tauri:', e);
      }
    }
    return () => {};
  },

  listenSessionReady: async (callback) => {
    if (Bridge.isTauri()) {
      try {
        return await listen('session-ready', (event) => {
          if (event.payload && event.payload.session_id) {
            callback(event.payload.session_id);
          }
        });
      } catch (e) {
        console.warn('Error listening for session ready in Tauri:', e);
      }
    }
    return () => {};
  },

  listenWebviewClick: async (callback) => {
    if (Bridge.isTauri()) {
      try {
        return await listen('webview-clicked', () => {
          callback();
        });
      } catch (e) {
        console.warn('Error listening for webview click in Tauri:', e);
      }
    }
    return () => {};
  },

  showToast: async (message, toastType) => {
    if (Bridge.isTauri()) {
      try {
        return await invoke('show_toast', { message, toastType: toastType || 'success' });
      } catch (e) {
        console.error('Error in Tauri show_toast:', e);
        return { success: false, error: e };
      }
    }
    return { success: true };
  },
};
