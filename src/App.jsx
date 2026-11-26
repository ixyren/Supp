import React, { useState, useEffect, useRef, useCallback } from 'react';
import { motion as Motion, AnimatePresence } from 'framer-motion';
import { Bridge } from './bridge';
import { EASE_EMPHASIZED, DEFAULT_AVATAR } from './components/motionTokens';
import { IntroScreen } from './components/IntroScreen';
import { SidebarRail } from './components/SidebarRail';
import { PreferencesPanel } from './components/PreferencesPanel';
import { SessionWebview } from './components/SessionWebview';
import { SessionContextMenu } from './components/SessionContextMenu';
import { DeleteSessionModal } from './components/DeleteSessionModal';
import { ToastPill } from './components/ToastPill';

function loadSavedInstances() {
  const saved = localStorage.getItem('whatsapp_instances');
  if (!saved) return [];
  try {
    const parsed = JSON.parse(saved);
    if (!Array.isArray(parsed)) return [];
    return parsed.map((s) => ({
      ...s,
      avatar: s.avatar || DEFAULT_AVATAR,
      isReady: Boolean(s.isReady ?? !Bridge.isTauri()),
      hasError: false,
      errorMsg: null,
    }));
  } catch (e) {
    console.error('Error parsing saved instances:', e);
    return [];
  }
}

function getNextSessionNumber(currentInstances) {
  let maxNum = 0;
  for (const inst of currentInstances) {
    const match = typeof inst.name === 'string' && inst.name.match(/^Sesión\s+(\d+)$/i);
    if (match) {
      const n = parseInt(match[1], 10);
      if (n > maxNum) maxNum = n;
    }
  }
  return Math.max(maxNum + 1, currentInstances.length + 1);
}

function App() {
  const [instances, setInstances] = useState(loadSavedInstances);
  const [activeTab, setActiveTab] = useState(() =>
    instances.length > 0 ? instances[0].id : 'settings'
  );
  const [sidebarHovered, setSidebarHovered] = useState(false);
  const [hoverExpandEnabled, setHoverExpandEnabled] = useState(() => {
    const saved = localStorage.getItem('hover_expand_enabled');
    return saved !== null ? saved === 'true' : true;
  });

  // Startup / staggered Intro Screen state
  const [introVisible, setIntroVisible] = useState(true);
  const [introExiting, setIntroExiting] = useState(false);
  const [startedSessionIds, setStartedSessionIds] = useState(() => new Set());
  const [readySessionIds, setReadySessionIds] = useState(() => new Set());
  const [introSubProgress, setIntroSubProgress] = useState(0);
  const [allInitialSessionsReady, setAllInitialSessionsReady] = useState(false);

  // Interactive UI states
  const contextMenuRef = useRef(null);
  const [contextMenu, setContextMenu] = useState(null);
  const [panelTemporarilyOpen, setPanelTemporarilyOpen] = useState(false);
  const [deletingSessionId, setDeletingSessionId] = useState(null);
  const [editingSession, setEditingSession] = useState(null);
  const [editingName, setEditingName] = useState('');
  const [isAddingSession, setIsAddingSession] = useState(false);
  const [isDraggingSession, setIsDraggingSession] = useState(false);
  const [slideDirection, setSlideDirection] = useState(0);
  const [notification, setNotification] = useState(null);

  // System settings state
  const [startWithWindows, setStartWithWindows] = useState(
    () => localStorage.getItem('startWithWindows') === 'true'
  );
  const [startMinimized, setStartMinimized] = useState(
    () => localStorage.getItem('startMinimized') === 'true'
  );
  const [closeToTray, setCloseToTray] = useState(() => {
    const saved = localStorage.getItem('closeToTray');
    return saved !== null ? saved === 'true' : true;
  });

  const isSidebarExpanded =
    (sidebarHovered && (hoverExpandEnabled || panelTemporarilyOpen)) ||
    Boolean(contextMenu) ||
    Boolean(editingSession) ||
    Boolean(deletingSessionId) ||
    isDraggingSession;

  // Refs to eliminate stale closures & StrictMode side effects inside updaters
  const instancesRef = useRef(instances);
  const activeTabRef = useRef(activeTab);
  const introVisibleRef = useRef(introVisible);
  const isSidebarExpandedRef = useRef(isSidebarExpanded);
  const hoverExpandEnabledRef = useRef(hoverExpandEnabled);
  const editingSessionRef = useRef(editingSession);
  const deletingSessionRef = useRef(deletingSessionId);
  const isDraggingSessionRef = useRef(isDraggingSession);
  const isAddingSessionRef = useRef(isAddingSession);
  const introExitStartedRef = useRef(false);
  const introFinishedSentRef = useRef(false);

  useEffect(() => {
    instancesRef.current = instances;
  }, [instances]);

  useEffect(() => {
    activeTabRef.current = activeTab;
  }, [activeTab]);

  useEffect(() => {
    introVisibleRef.current = introVisible;
  }, [introVisible]);

  useEffect(() => {
    isSidebarExpandedRef.current = isSidebarExpanded;
  }, [isSidebarExpanded]);

  useEffect(() => {
    hoverExpandEnabledRef.current = hoverExpandEnabled;
  }, [hoverExpandEnabled]);

  useEffect(() => {
    editingSessionRef.current = editingSession;
  }, [editingSession]);

  useEffect(() => {
    deletingSessionRef.current = deletingSessionId;
  }, [deletingSessionId]);

  useEffect(() => {
    isDraggingSessionRef.current = isDraggingSession;
  }, [isDraggingSession]);

  // Persist instances to localStorage
  useEffect(() => {
    localStorage.setItem('whatsapp_instances', JSON.stringify(instances));
  }, [instances]);

  // Sync Tauri native sidebar bounds automatically whenever expansion or active tab changes
  useEffect(() => {
    if (Bridge.isTauri() && !introVisible) {
      Bridge.updateSidebarBounds(activeTab, isSidebarExpanded ? 280.0 : 80.0);
    }
  }, [isSidebarExpanded, activeTab, introVisible]);

  const triggerNotification = useCallback((notif) => {
    setNotification(notif);
    if (notif && Bridge.isTauri()) {
      Bridge.showToast(notif.message, notif.type);
    }
  }, []);

  // Auto-hide toast notifications
  useEffect(() => {
    if (!notification) return;
    const duration = notification.type === 'loading' ? 3500 : 3000;
    const timer = setTimeout(() => setNotification(null), duration);
    return () => clearTimeout(timer);
  }, [notification]);

  const handleUpdateSession = useCallback((sessionId, updates) => {
    setInstances((prev) =>
      prev.map((s) => (s.id === sessionId ? { ...s, ...updates } : s))
    );
  }, []);

  const handleSessionReady = useCallback((sessionId) => {
    setInstances((prev) => {
      const target = prev.find((s) => s.id === sessionId);
      if (!target || (target.isReady && !target.hasError)) return prev;
      return prev.map((s) =>
        s.id === sessionId ? { ...s, isReady: true, hasError: false, errorMsg: null } : s
      );
    });
  }, []);

  const handleSessionLoadFail = useCallback(
    (sessionId, err) => {
      console.error(`Session ${sessionId} load fail:`, err);
      setInstances((prev) =>
        prev.map((s) =>
          s.id === sessionId ? { ...s, isReady: true, hasError: true, errorMsg: err } : s
        )
      );
      triggerNotification({
        message: 'Error de conexión. Presiona Ctrl + R para recargar.',
        type: 'error',
      });
    },
    [triggerNotification]
  );

  // Close context menu on outside click, Escape, or window blur
  useEffect(() => {
    if (!contextMenu) return;

    const handlePointerDown = (e) => {
      if (contextMenuRef.current && contextMenuRef.current.contains(e.target)) {
        return;
      }
      setContextMenu(null);
      setPanelTemporarilyOpen(false);
    };

    const handleKeyDown = (e) => {
      if (e.key === 'Escape') {
        setContextMenu(null);
        setPanelTemporarilyOpen(false);
      }
    };

    const handleBlur = () => {
      setContextMenu(null);
      setPanelTemporarilyOpen(false);
      setSidebarHovered(false);
    };

    document.addEventListener('pointerdown', handlePointerDown, true);
    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('blur', handleBlur);

    return () => {
      document.removeEventListener('pointerdown', handlePointerDown, true);
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('blur', handleBlur);
    };
  }, [contextMenu]);

  // Listen for clicks inside WhatsApp WebView2: close ALL sidebar overlays & collapse cleanly
  useEffect(() => {
    if (!Bridge.isTauri()) return;
    let cancelled = false;
    let unlisten = null;

    Bridge.listenWebviewClick(() => {
      setContextMenu(null);
      setEditingSession(null);
      setEditingName('');
      setDeletingSessionId(null);
      setPanelTemporarilyOpen(false);
      setSidebarHovered(false);
      editingSessionRef.current = null;
      deletingSessionRef.current = null;
      isSidebarExpandedRef.current = false;

      const currentTab = activeTabRef.current;
      if (!introVisibleRef.current && currentTab !== 'settings') {
        Bridge.updateSidebarBounds(currentTab, 80.0);
      }
    }).then((fn) => {
      if (cancelled) {
        if (typeof fn === 'function') fn();
        return;
      }
      if (typeof fn === 'function') unlisten = fn;
    });

    return () => {
      cancelled = true;
      if (typeof unlisten === 'function') unlisten();
    };
  }, []);

  // Listen for WhatsApp unread count updates (registered once with proper cleanup)
  useEffect(() => {
    if (!Bridge.isTauri()) return;
    let cancelled = false;
    let unlisten = null;

    Bridge.listenUnread((sessionId, unread) => {
      handleUpdateSession(sessionId, { unread });
    }).then((fn) => {
      if (cancelled) {
        if (typeof fn === 'function') fn();
        return;
      }
      if (typeof fn === 'function') unlisten = fn;
    });

    return () => {
      cancelled = true;
      if (typeof unlisten === 'function') unlisten();
    };
  }, [handleUpdateSession]);

  // Listen for session-ready events beyond startup (for newly added or reloaded sessions)
  useEffect(() => {
    if (!Bridge.isTauri()) return;
    let cancelled = false;
    let unlisten = null;

    Bridge.listenSessionReady((sessionId) => {
      handleSessionReady(sessionId);
    }).then((fn) => {
      if (cancelled) {
        if (typeof fn === 'function') fn();
        return;
      }
      if (typeof fn === 'function') unlisten = fn;
    });

    return () => {
      cancelled = true;
      if (typeof unlisten === 'function') unlisten();
    };
  }, [handleSessionReady]);

  // Ensure abrupt pointer exit out of main WebView collapses the sidebar cleanly
  useEffect(() => {
    const handleMouseOut = (e) => {
      if (!e.relatedTarget && !e.toElement) {
        setSidebarHovered(false);
        setPanelTemporarilyOpen(false);
        const keepExpanded =
          Boolean(contextMenu) ||
          Boolean(editingSessionRef.current) ||
          Boolean(deletingSessionRef.current) ||
          isDraggingSessionRef.current;
        isSidebarExpandedRef.current = keepExpanded;
        if (
          Bridge.isTauri() &&
          !introVisibleRef.current &&
          activeTabRef.current !== 'settings'
        ) {
          Bridge.updateSidebarBounds(activeTabRef.current, keepExpanded ? 280.0 : 80.0);
        }
      }
    };
    window.addEventListener('mouseout', handleMouseOut);
    return () => window.removeEventListener('mouseout', handleMouseOut);
  }, [contextMenu]);

  // Prevent Ctrl+R / F5 from accidentally reloading the main Tauri shell UI; reload active session if supported
  useEffect(() => {
    if (!Bridge.isTauri()) return;
    const handleKeyDown = (e) => {
      const isReloadKey =
        ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'r') || e.key === 'F5';
      if (!isReloadKey) return;

      e.preventDefault();
      const currentTab = activeTabRef.current;
      if (
        currentTab !== 'settings' &&
        !introVisibleRef.current &&
        typeof Bridge.reloadSession === 'function'
      ) {
        triggerNotification({ message: 'Recargando sesión...', type: 'loading' });
        Bridge.reloadSession(currentTab)
          .then(() => {
            handleUpdateSession(currentTab, { hasError: false, errorMsg: null });
          })
          .catch((err) => {
            console.error('Error reloading session:', err);
            triggerNotification({ message: 'No se pudo recargar la sesión', type: 'error' });
          });
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleUpdateSession, triggerNotification]);

  // Load persisted Tauri / system settings on mount
  useEffect(() => {
    let cancelled = false;
    Bridge.getSettings().then((s) => {
      if (cancelled || !s) return;
      if (s.openAtLogin !== undefined) setStartWithWindows(s.openAtLogin);
      if (s.startMinimized !== undefined) setStartMinimized(s.startMinimized);
      if (s.closeToTray !== undefined) setCloseToTray(s.closeToTray);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  // Staggered startup (125ms per session) during IntroScreen
  useEffect(() => {
    let cancelled = false;
    let unlistenReady = null;
    let subTicker = null;
    const readyResolvers = new Map();
    const alreadyReported = new Set();
    const initialInstances = instancesRef.current;
    const initialActiveTab = activeTabRef.current;

    const waitMs = (ms) => new Promise((res) => setTimeout(res, ms));

    const waitForSessionDomReady = (sessionId, timeoutMs = 6500) => {
      if (alreadyReported.has(sessionId)) {
        return Promise.resolve();
      }
      return new Promise((resolve) => {
        const timer = setTimeout(() => {
          readyResolvers.delete(sessionId);
          resolve();
        }, timeoutMs);
        readyResolvers.set(sessionId, () => {
          clearTimeout(timer);
          readyResolvers.delete(sessionId);
          resolve();
        });
      });
    };

    const runStaggeredStartup = async () => {
      if (Bridge.isTauri()) {
        const fn = await Bridge.listenSessionReady((sessionId) => {
          alreadyReported.add(sessionId);
          const resolver = readyResolvers.get(sessionId);
          if (resolver) resolver();
        });
        if (cancelled) {
          if (typeof fn === 'function') fn();
          return;
        }
        if (typeof fn === 'function') unlistenReady = fn;
      }

      if (initialInstances.length === 0) {
        setIntroSubProgress(0.5);
        await waitMs(320);
        if (cancelled) return;
        setIntroSubProgress(1);
        setAllInitialSessionsReady(true);
        return;
      }

      subTicker = setInterval(() => {
        setIntroSubProgress((prev) => {
          if (prev >= 0.86) return prev;
          const step = (0.86 - prev) * 0.16 + 0.025;
          return Math.min(0.86, prev + step);
        });
      }, 75);

      let lastCompleteTime = 0;
      const markReadyStaggered = async (sessionId) => {
        const now = performance.now();
        const waitNeeded = Math.max(0, lastCompleteTime + 125 - now);
        lastCompleteTime = now + waitNeeded;
        if (waitNeeded > 0) {
          await waitMs(waitNeeded);
        }
        if (cancelled) return;
        setReadySessionIds((prev) => {
          const next = new Set(prev);
          next.add(sessionId);
          return next;
        });
        handleSessionReady(sessionId);
      };

      const sessionPromises = initialInstances.map(async (inst, idx) => {
        if (idx > 0) {
          await waitMs(idx * 125);
        }
        if (cancelled) return;

        setStartedSessionIds((prev) => {
          const next = new Set(prev);
          next.add(inst.id);
          return next;
        });

        if (Bridge.isTauri()) {
          const readyPromise = waitForSessionDomReady(inst.id, 6500);
          const minPace = waitMs(350 + idx * 125);
          try {
            await Bridge.createSessionWebview(inst.id, 80.0, inst.id === initialActiveTab);
            await Promise.all([readyPromise, minPace]);
          } catch (err) {
            console.error(`Error creating startup session ${inst.id}:`, err);
            const resolver = readyResolvers.get(inst.id);
            if (resolver) resolver();
            handleUpdateSession(inst.id, { hasError: true, errorMsg: String(err) });
            await minPace;
          }
        } else {
          await waitMs(350 + idx * 125);
        }

        await markReadyStaggered(inst.id);
      });

      await Promise.all(sessionPromises);
      clearInterval(subTicker);

      if (!cancelled) {
        setIntroSubProgress(1);
        setAllInitialSessionsReady(true);
      }
    };

    runStaggeredStartup();

    return () => {
      cancelled = true;
      clearInterval(subTicker);
      for (const resolver of readyResolvers.values()) {
        resolver();
      }
      readyResolvers.clear();
      if (typeof unlistenReady === 'function') unlistenReady();
    };
  }, [handleSessionReady, handleUpdateSession]);

  // Start IntroScreen exit fade once all initial sessions report ready
  useEffect(() => {
    if (!allInitialSessionsReady || introExitStartedRef.current) return;
    introExitStartedRef.current = true;
    let finishTimer;
    const exitTimer = setTimeout(() => {
      setIntroExiting(true);
      finishTimer = setTimeout(() => {
        introVisibleRef.current = false;
        setIntroVisible(false);
      }, 180);
    }, 220);

    return () => {
      clearTimeout(exitTimer);
      clearTimeout(finishTimer);
    };
  }, [allInitialSessionsReady]);

  // Trigger native finishIntro AFTER introVisible flips to false (cannot be cancelled by introVisible cleanup)
  useEffect(() => {
    if (!introVisible && allInitialSessionsReady && !introFinishedSentRef.current) {
      introFinishedSentRef.current = true;
      if (Bridge.isTauri()) {
        const liveW = isSidebarExpandedRef.current ? 280.0 : 80.0;
        Bridge.finishIntro(
          activeTabRef.current,
          instancesRef.current.map((i) => i.id),
          liveW
        ).catch((err) => {
          console.error('Error finishing intro:', err);
        });
      }
    }
  }, [introVisible, allInitialSessionsReady]);

  const getTabIndex = useCallback((id, list = instancesRef.current) => {
    return id === 'settings' ? list.length : list.findIndex((i) => i.id === id);
  }, []);

  const handleTabChange = useCallback(
    (targetTabId) => {
      if (targetTabId === activeTabRef.current) return;
      const list = instancesRef.current;
      const prevIdx = getTabIndex(activeTabRef.current, list);
      const nextIdx = getTabIndex(targetTabId, list);
      const dir = nextIdx > prevIdx ? 1 : -1;
      const prevTab = activeTabRef.current;

      setContextMenu(null);
      setPanelTemporarilyOpen(false);
      setSlideDirection(dir);
      activeTabRef.current = targetTabId;
      setActiveTab(targetTabId);

      if (targetTabId === 'settings') {
        setNotification(null);
      }

      if (Bridge.isTauri()) {
        const liveW = isSidebarExpandedRef.current ? 280.0 : 80.0;
        Bridge.switchSession(
          targetTabId,
          list.map((i) => i.id),
          liveW,
          dir,
          prevTab
        ).catch((err) => {
          console.error('Error switching session:', err);
        });
      }
    },
    [getTabIndex]
  );

  const handleReorder = useCallback((newOrder) => {
    instancesRef.current = newOrder;
    setInstances(newOrder);
    localStorage.setItem('whatsapp_instances', JSON.stringify(newOrder));
  }, []);

  const handleMouseEnter = useCallback(() => {
    if (introVisibleRef.current) return;
    setSidebarHovered(true);
    if (hoverExpandEnabledRef.current || panelTemporarilyOpen) {
      isSidebarExpandedRef.current = true;
      if (Bridge.isTauri() && activeTabRef.current !== 'settings') {
        Bridge.updateSidebarBounds(activeTabRef.current, 280.0);
      }
    }
  }, [panelTemporarilyOpen]);

  const handleMouseLeave = useCallback(() => {
    setSidebarHovered(false);
    setPanelTemporarilyOpen(false);
    const keepExpanded =
      Boolean(contextMenu) ||
      Boolean(editingSessionRef.current) ||
      Boolean(deletingSessionRef.current) ||
      isDraggingSessionRef.current;
    isSidebarExpandedRef.current = keepExpanded;
    if (
      Bridge.isTauri() &&
      !introVisibleRef.current &&
      activeTabRef.current !== 'settings'
    ) {
      Bridge.updateSidebarBounds(activeTabRef.current, keepExpanded ? 280.0 : 80.0);
    }
  }, [contextMenu]);

  // Pure value update with zero side effects inside setState updater (safe in StrictMode)
  const toggleHoverExpand = useCallback(
    (e, forcedValue) => {
      e?.stopPropagation();
      const next =
        typeof forcedValue === 'boolean' ? forcedValue : !hoverExpandEnabledRef.current;
      hoverExpandEnabledRef.current = next;
      setHoverExpandEnabled(next);
      localStorage.setItem('hover_expand_enabled', String(next));

      if (Bridge.isTauri() && activeTabRef.current !== 'settings') {
        const nextExpanded =
          (sidebarHovered && (next || panelTemporarilyOpen)) ||
          Boolean(contextMenu) ||
          Boolean(editingSessionRef.current) ||
          Boolean(deletingSessionRef.current) ||
          isDraggingSessionRef.current;
        isSidebarExpandedRef.current = nextExpanded;
        Bridge.updateSidebarBounds(activeTabRef.current, nextExpanded ? 280.0 : 80.0);
      }

      triggerNotification({
        message: next
          ? 'Despliegue al pasar el mouse: Activado'
          : 'Despliegue al pasar el mouse: Desactivado',
        type: 'success',
      });
    },
    [sidebarHovered, panelTemporarilyOpen, contextMenu, triggerNotification]
  );

  const handleAddInstance = useCallback(async () => {
    if (isAddingSessionRef.current) return;
    isAddingSessionRef.current = true;
    setIsAddingSession(true);

    const currentList = instancesRef.current;
    const sessionNumber = getNextSessionNumber(currentList);
    const newInstance = {
      id: crypto.randomUUID ? crypto.randomUUID() : `session_${Date.now()}`,
      name: `Sesión ${sessionNumber}`,
      avatar: DEFAULT_AVATAR,
      unread: 0,
      isReady: !Bridge.isTauri(),
      hasError: false,
      errorMsg: null,
    };

    const updatedInstances = [...currentList, newInstance];
    instancesRef.current = updatedInstances;
    setInstances(updatedInstances);
    localStorage.setItem('whatsapp_instances', JSON.stringify(updatedInstances));

    const prevTab = activeTabRef.current;
    setContextMenu(null);
    setSlideDirection(1);
    activeTabRef.current = newInstance.id;
    setActiveTab(newInstance.id);

    triggerNotification({
      message: 'Preparando entorno aislado para nueva sesión...',
      type: 'loading',
    });

    try {
      if (Bridge.isTauri()) {
        const liveW = isSidebarExpandedRef.current ? 280.0 : 80.0;
        await Bridge.createSessionWebview(newInstance.id, liveW, false);
        const latestList = instancesRef.current;
        const finalW = isSidebarExpandedRef.current ? 280.0 : 80.0;
        await Bridge.switchSession(
          newInstance.id,
          latestList.map((i) => i.id),
          finalW,
          1,
          prevTab
        );
        // Re-sync in case pointer left sidebar while webview creation/switch was in flight
        const postW = isSidebarExpandedRef.current ? 280.0 : 80.0;
        if (activeTabRef.current === newInstance.id) {
          Bridge.updateSidebarBounds(newInstance.id, postW);
        }
      }
      triggerNotification({ message: 'Sesión iniciada correctamente', type: 'success' });
    } catch (e) {
      console.error('Error adding session:', e);
      handleUpdateSession(newInstance.id, { hasError: true, errorMsg: String(e) });
      triggerNotification({ message: 'Error iniciando sesión', type: 'error' });
    } finally {
      isAddingSessionRef.current = false;
      setIsAddingSession(false);
    }
  }, [handleUpdateSession, triggerNotification]);

  const executeDeleteSession = useCallback(
    async (id) => {
      if (!id) return;
      const currentList = instancesRef.current;
      const deletedIdx = currentList.findIndex((i) => i.id === id);
      const newInst = currentList.filter((i) => i.id !== id);

      instancesRef.current = newInst;
      setInstances(newInst);
      localStorage.setItem('whatsapp_instances', JSON.stringify(newInst));

      if (editingSessionRef.current === id) {
        editingSessionRef.current = null;
        setEditingSession(null);
        setEditingName('');
      }
      setContextMenu((prev) => (prev?.session?.id === id ? null : prev));
      deletingSessionRef.current = null;
      setDeletingSessionId(null);

      if (activeTabRef.current === id) {
        const fallbackSession =
          newInst.length > 0
            ? newInst[Math.min(Math.max(deletedIdx, 0), newInst.length - 1)]
            : null;
        const nextTab = fallbackSession ? fallbackSession.id : 'settings';
        const nextIdx =
          nextTab === 'settings'
            ? newInst.length
            : newInst.findIndex((i) => i.id === nextTab);
        const dir = nextIdx >= deletedIdx ? 1 : -1;

        setSlideDirection(dir);
        activeTabRef.current = nextTab;
        setActiveTab(nextTab);

        if (Bridge.isTauri()) {
          const keepExpanded =
            Boolean(editingSessionRef.current) ||
            (sidebarHovered && (hoverExpandEnabledRef.current || panelTemporarilyOpen));
          const liveW = keepExpanded ? 280.0 : 80.0;
          try {
            await Bridge.switchSession(
              nextTab,
              newInst.map((i) => i.id),
              liveW,
              dir,
              id
            );
            Bridge.updateSidebarBounds(nextTab, liveW);
          } catch (err) {
            console.error('Error switching after delete:', err);
          }
        }
      }

      if (Bridge.isTauri()) {
        try {
          await Bridge.destroySession(id);
        } catch (err) {
          console.error('Error destroying session:', err);
          triggerNotification({
            message: 'Sesión quitada, pero hubo un error limpiando archivos',
            type: 'error',
          });
          return;
        }
      }
      triggerNotification({ message: 'Sesión eliminada correctamente', type: 'success' });
    },
    [sidebarHovered, panelTemporarilyOpen, triggerNotification]
  );

  const handleToggleSetting = useCallback(
    async (name, value) => {
      try {
        switch (name) {
          case 'startWithWindows':
            setStartWithWindows(value);
            await Bridge.toggleStartup(value);
            break;
          case 'startMinimized':
            setStartMinimized(value);
            await Bridge.setStartMinimized(value);
            break;
          case 'closeToTray':
            setCloseToTray(value);
            await Bridge.setCloseToTray(value);
            break;
          case 'hoverExpand':
            toggleHoverExpand(null, value);
            break;
          default:
            break;
        }
      } catch (err) {
        console.error(`Error updating setting ${name}:`, err);
        if (name === 'startWithWindows') setStartWithWindows(!value);
        if (name === 'startMinimized') setStartMinimized(!value);
        if (name === 'closeToTray') setCloseToTray(!value);
        triggerNotification({ message: 'No se pudo guardar la configuración', type: 'error' });
      }
    },
    [toggleHoverExpand, triggerNotification]
  );

  const startEditingName = useCallback((session) => {
    deletingSessionRef.current = null;
    setDeletingSessionId(null);
    editingSessionRef.current = session.id;
    setEditingSession(session.id);
    setEditingName(session.name);
  }, []);

  const saveEditingName = useCallback(() => {
    if (editingSessionRef.current && editingName.trim()) {
      handleUpdateSession(editingSessionRef.current, { name: editingName.trim() });
    }
    editingSessionRef.current = null;
    setEditingSession(null);
    setEditingName('');
  }, [editingName, handleUpdateSession]);

  const cancelEditingName = useCallback(() => {
    editingSessionRef.current = null;
    setEditingSession(null);
    setEditingName('');
  }, []);

  const requestDeleteSession = useCallback((id) => {
    setContextMenu(null);
    editingSessionRef.current = null;
    setEditingSession(null);
    setEditingName('');
    deletingSessionRef.current = id;
    setDeletingSessionId(id);
  }, []);

  const handleReloadSession = useCallback(
    (sessionId) => {
      if (Bridge.isTauri() && typeof Bridge.reloadSession === 'function') {
        triggerNotification({ message: 'Recargando sesión...', type: 'loading' });
        Bridge.reloadSession(sessionId)
          .then(() => {
            handleUpdateSession(sessionId, { hasError: false, errorMsg: null });
          })
          .catch((err) => {
            console.error('Error reloading session:', err);
            triggerNotification({ message: 'No se pudo recargar la sesión', type: 'error' });
          });
      }
    },
    [handleUpdateSession, triggerNotification]
  );

  const canReloadSession = !Bridge.isTauri()
    ? false
    : typeof Bridge.reloadSession === 'function';

  const currentIndex = getTabIndex(activeTab, instances);

  const getWebviewVariants = (id) => {
    const idx = instances.findIndex((i) => i.id === id);
    const isActive = activeTab === id;
    if (isActive) {
      return { y: 0, opacity: 1, zIndex: 10, transition: { duration: 0.34, ease: EASE_EMPHASIZED } };
    }
    return idx < currentIndex
      ? { y: '-100%', opacity: 0, zIndex: 0, transition: { duration: 0.34, ease: EASE_EMPHASIZED } }
      : { y: '100%', opacity: 0, zIndex: 0, transition: { duration: 0.34, ease: EASE_EMPHASIZED } };
  };

  const deletingSessionObj = deletingSessionId
    ? instances.find((i) => i.id === deletingSessionId) || null
    : null;

  return (
    <div
      onContextMenu={(e) => {
        const tag = e.target?.tagName;
        if (tag !== 'INPUT' && tag !== 'TEXTAREA') {
          e.preventDefault();
        }
      }}
      className={`flex h-screen w-screen overflow-hidden ${
        activeTab === 'settings' ? 'bg-[#12150e]' : 'bg-[#181c13]'
      } text-[#e7ebdc] font-sans antialiased`}
    >
      {introVisible && (
        <IntroScreen
          instances={instances}
          startedSessionIds={startedSessionIds}
          readySessionIds={readySessionIds}
          subProgress={introSubProgress}
          allReady={allInitialSessionsReady}
          exiting={introExiting}
        />
      )}

      <SidebarRail
        activeTab={activeTab}
        instances={instances}
        isSidebarExpanded={isSidebarExpanded}
        hoverExpandEnabled={hoverExpandEnabled}
        editingSession={editingSession}
        deletingSessionId={deletingSessionId}
        editingName={editingName}
        setEditingName={setEditingName}
        isAddingSession={isAddingSession}
        onMouseEnter={handleMouseEnter}
        onMouseLeave={handleMouseLeave}
        onToggleHoverExpand={toggleHoverExpand}
        onTabChange={handleTabChange}
        onReorder={handleReorder}
        onOpenContextMenu={(session, x, y) => {
          setPanelTemporarilyOpen(true);
          setSidebarHovered(true);
          setContextMenu({ session, x, y });
        }}
        onStartEditing={startEditingName}
        onSaveEditing={saveEditingName}
        onCancelEditing={cancelEditingName}
        onRequestDelete={requestDeleteSession}
        onAddInstance={handleAddInstance}
        onDragStateChange={(dragging) => {
          isDraggingSessionRef.current = dragging;
          setIsDraggingSession(dragging);
        }}
      />

      {/* Main Content Area (Settings & Empty-State Dashboard) */}
      <AnimatePresence mode="wait">
        {activeTab === 'settings' && (
          <Motion.main
            key="settings-main"
            initial={{ opacity: 0, y: slideDirection > 0 ? 28 : -28, scale: 0.985 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: slideDirection > 0 ? -20 : 20, scale: 0.985 }}
            transition={{ duration: 0.24, ease: EASE_EMPHASIZED }}
            style={{ willChange: 'transform, opacity' }}
            className="flex-1 h-full relative overflow-hidden bg-[#12150e]"
          >
            <PreferencesPanel
              settings={{
                openAtLogin: startWithWindows,
                startMinimized: startMinimized,
                closeToTray: closeToTray,
                hoverExpandEnabled: hoverExpandEnabled,
              }}
              instances={instances}
              isAddingSession={isAddingSession}
              canReloadSession={canReloadSession}
              toggleSetting={handleToggleSetting}
              onAddInstance={handleAddInstance}
              onSelectSession={handleTabChange}
              onRenameSession={(id, newName) => handleUpdateSession(id, { name: newName })}
              onReloadSession={handleReloadSession}
              onDeleteSession={requestDeleteSession}
            />
          </Motion.main>
        )}
      </AnimatePresence>

      {/* Non-Tauri fallback webviews (Electron or Browser dev preview) */}
      {!Bridge.isTauri() &&
        instances.map((instance) => (
          <SessionWebview
            key={instance.id}
            session={instance}
            isActive={activeTab === instance.id}
            onReady={handleSessionReady}
            onUpdateSession={handleUpdateSession}
            onLoadFail={handleSessionLoadFail}
            animationVariants={getWebviewVariants(instance.id)}
          />
        ))}

      {/* Toast Pill (shown inside Settings view; in Tauri sessions, Rust shows native floating toast inside WebView2) */}
      <ToastPill notification={activeTab === 'settings' ? notification : null} />

      {/* Session Context Menu */}
      <SessionContextMenu
        contextMenu={contextMenu}
        menuRef={contextMenuRef}
        activeTab={activeTab}
        isCompactViewport={activeTab !== 'settings'}
        canReloadSession={canReloadSession}
        onOpenSession={(sess) => {
          setContextMenu(null);
          setPanelTemporarilyOpen(false);
          handleTabChange(sess.id);
        }}
        onRenameSession={(sess) => {
          setContextMenu(null);
          startEditingName(sess);
        }}
        onReloadSession={(sess) => {
          setContextMenu(null);
          setPanelTemporarilyOpen(false);
          handleReloadSession(sess.id);
        }}
        onDeleteSession={(sess) => {
          requestDeleteSession(sess.id);
        }}
      />

      {/* Delete Confirmation Modal (adapts cleanly to 280px rail over active session OR 100vw in Settings) */}
      <DeleteSessionModal
        session={deletingSessionObj}
        isCompactViewport={activeTab !== 'settings'}
        onConfirm={executeDeleteSession}
        onCancel={() => {
          deletingSessionRef.current = null;
          setDeletingSessionId(null);
          setPanelTemporarilyOpen(false);
        }}
      />
    </div>
  );
}

export default App;
