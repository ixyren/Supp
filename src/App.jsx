// Updated App.jsx with fixed sidebar heights, AnimatePresence wrapper, and main.js menu bar removal
import React, { useState, useEffect, useRef } from 'react';
import { MessageSquare, Settings, Plus, Trash2, AlertTriangle, CheckCircle, Pencil } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';

// Default avatar for new sessions
const DEFAULT_AVATAR = 'https://cdn.pixabay.com/photo/2015/10/05/22/37/blank-profile-picture-973460_1280.png';

// Sidebar animation variants
const sidebarVariants = {
  open: {
    width: '18rem',
    transition: { type: 'spring', stiffness: 300, damping: 30 }
  },
  closed: {
    width: '5rem',
    transition: { type: 'spring', stiffness: 300, damping: 30, delay: 0.1 }
  }
};

// Text animation for sidebar open/close (wrapper)
const textWrapperVariants = {
  open: { opacity: 1, width: 'auto', transition: { duration: 0.2 } },
  closed: { opacity: 0, width: 0, transition: { duration: 0.1 } }
};

// Text animation variants for individual elements
const textVariants = {
  visible: {
    opacity: 1,
    width: 'auto',
    transition: { duration: 0.3, delay: 0.1, ease: 'easeOut' }
  },
  hidden: {
    opacity: 0,
    width: 0,
    transition: { duration: 0.1, ease: 'easeIn' }
  }
};

// SessionWebview Component - Handles Electron webview events properly
function SessionWebview({ session, isActive, onReady, onUpdateSession, onLoadFail, animationVariants }) {
  const webviewRef = useRef(null);

  useEffect(() => {
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
          // Keep existing name unless title is a custom name
          name: (unreadCount === 0 && title !== 'WhatsApp' && title !== 'WhatsApp Web') ? title : session.name
        });
      } catch (e) { console.error(e); }
    };

    const handleFailLoad = (e) => {
      if (e.isMainFrame && e.errorCode !== -3) {
        onLoadFail(session.id, e.errorDescription);
      }
    };

    webview.addEventListener('dom-ready', handleDomReady);
    webview.addEventListener('page-title-updated', handleTitleUpdate);
    webview.addEventListener('did-fail-load', handleFailLoad);

    const failsafeTimeout = setTimeout(() => {
      if (!session.isReady) {
        console.warn(`Failsafe timeout for ${session.id}`);
        onReady(session.id);
      }
    }, 15000);

    const avatarInterval = setInterval(() => {
      if (session.isReady) {
        webview.executeJavaScript(`
          (function(){
            const img = document.querySelector('header img');
            return img ? img.src : null;
          })();
        `).then(src => {
          if (src && src !== session.avatar) {
            onUpdateSession(session.id, { avatar: src });
          }
        }).catch(() => { });
      }
    }, 5000);

    return () => {
      clearTimeout(failsafeTimeout);
      clearInterval(avatarInterval);
      webview.removeEventListener('dom-ready', handleDomReady);
      webview.removeEventListener('page-title-updated', handleTitleUpdate);
      webview.removeEventListener('did-fail-load', handleFailLoad);
    };
  }, [session.id, session.isReady, session.avatar, onReady, onUpdateSession, onLoadFail]);

  return (
    <motion.div
      animate={animationVariants}
      className={`absolute inset-0 w-full h-full transition-opacity duration-300 ${isActive ? 'opacity-100' : 'opacity-0'}`}
      style={{ pointerEvents: isActive ? 'auto' : 'none' }}
    >
      <webview
        ref={webviewRef}
        src="https://web.whatsapp.com"
        partition={`persist:whatsapp_${session.id}`}
        allowpopups="true"
        webpreferences="contextIsolation=no"
        useragent="Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
        className="w-full h-full"
      />
    </motion.div>
  );
}

// Toggle Component
const Toggle = ({ active, onClick }) => (
  <div
    onClick={onClick}
    className={`w-12 h-6 rounded-full cursor-pointer transition-colors duration-300 relative ${active ? 'bg-[#39ff14]' : 'bg-gray-600'}`}
  >
    <motion.div
      layout
      className={`absolute top-1 w-4 h-4 rounded-full bg-white shadow-sm ${active ? 'right-1' : 'left-1'}`}
    />
  </div>
);

// PreferencesPanel Component
const PreferencesPanel = ({ settings, toggleSetting }) => {
  return (
    <motion.div
      initial={{ opacity: 0, scale: 0.95 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.95 }}
      transition={{ duration: 0.3, ease: "easeOut" }}
      className="absolute inset-0 z-50 flex flex-col items-center justify-center bg-[#0a0a0a] text-white"
    >
      {/* Background Gradient */}
      <div className="absolute inset-0 bg-[radial-gradient(circle_at_center,rgba(57,255,20,0.08)_0%,transparent_70%)] pointer-events-none" />

      {/* Header */}
      <div className="flex flex-col items-center mb-12 z-10">
        <div className="mb-4">
          <Settings size={64} className="text-[#39ff14]" strokeWidth={1.5} />
        </div>
        <h2 className="text-2xl font-bold mb-1">Preferencias del Sistema</h2>
        <p className="text-gray-500">Configuración de inicio y comportamiento</p>
      </div>

      {/* Options Grid */}
      <motion.div
        variants={{ show: { transition: { staggerChildren: 0.1 } } }}
        initial="hidden"
        animate="show"
        className="max-w-2xl w-full space-y-4 z-10 px-6"
      >
        {/* Toggle 1 */}
        <motion.div
          variants={{ hidden: { opacity: 0, x: -20 }, show: { opacity: 1, x: 0 } }}
          whileHover={{ scale: 1.02 }}
          className="bg-[#151515] border border-white/5 rounded-xl p-5 flex items-center justify-between hover:border-white/10 transition-colors cursor-pointer"
        >
          <div className="pr-4">
            <h3 className="font-medium text-white text-lg">Iniciar con Windows</h3>
            <p className="text-sm text-gray-400 mt-1">La aplicación se iniciará automáticamente al encender el sistema</p>
          </div>
          <Toggle
            active={settings.openAtLogin}
            onClick={() => toggleSetting('startWithWindows', !settings.openAtLogin)}
          />
        </motion.div>

        {/* Toggle 2 */}
        <motion.div
          variants={{ hidden: { opacity: 0, x: -20 }, show: { opacity: 1, x: 0 } }}
          whileHover={{ scale: 1.02 }}
          className="bg-[#151515] border border-white/5 rounded-xl p-5 flex items-center justify-between hover:border-white/10 transition-colors cursor-pointer"
        >
          <div className="pr-4">
            <h3 className="font-medium text-white text-lg">Iniciar minimizado en la bandeja del sistema</h3>
            <p className="text-sm text-gray-400 mt-1">La aplicación se iniciará oculta en la bandeja de notificaciones</p>
          </div>
          <Toggle
            active={settings.startMinimized}
            onClick={() => toggleSetting('startMinimized', !settings.startMinimized)}
          />
        </motion.div>

        {/* Toggle 3 */}
        <motion.div
          variants={{ hidden: { opacity: 0, x: -20 }, show: { opacity: 1, x: 0 } }}
          whileHover={{ scale: 1.02 }}
          className="bg-[#151515] border border-white/5 rounded-xl p-5 flex items-center justify-between hover:border-white/10 transition-colors cursor-pointer"
        >
          <div className="pr-4">
            <h3 className="font-medium text-white text-lg">Botón cerrar minimiza a la bandeja</h3>
            <p className="text-sm text-gray-400 mt-1">Al cerrar la ventana, la aplicación permanecerá en la bandeja del sistema</p>
          </div>
          <Toggle
            active={settings.closeToTray}
            onClick={() => toggleSetting('closeToTray', !settings.closeToTray)}
          />
        </motion.div>
      </motion.div>

      {/* Footer */}
      <div className="mt-auto mb-10 text-gray-500 text-sm z-10">
        Supp by Javier Medina
      </div>
    </motion.div>
  );
};

function App() {
  // Load instances from localStorage or start empty
  const [instances, setInstances] = useState(() => {
    const saved = localStorage.getItem('whatsapp_instances');
    return saved ? JSON.parse(saved) : [];
  });

  const [activeTab, setActiveTab] = useState(instances.length > 0 ? instances[0].id : 'settings');
  const [sidebarHovered, setSidebarHovered] = useState(false);

  const previousTabRef = useRef(activeTab);

  const [deleteModal, setDeleteModal] = useState(null);
  const [notification, setNotification] = useState(null);

  const [editingSession, setEditingSession] = useState(null);
  const [editingName, setEditingName] = useState('');

  const [startWithWindows, setStartWithWindows] = useState(() => {
    const saved = localStorage.getItem('startWithWindows');
    return saved === 'true';
  });
  const [startMinimized, setStartMinimized] = useState(() => {
    const saved = localStorage.getItem('startMinimized');
    return saved === 'true';
  });
  const [closeToTray, setCloseToTray] = useState(() => {
    const saved = localStorage.getItem('closeToTray');
    return saved !== null ? saved === 'true' : true;
  });

  // Save to localStorage
  useEffect(() => {
    localStorage.setItem('whatsapp_instances', JSON.stringify(instances));
  }, [instances]);

  // Auto‑hide notifications (except loading)
  useEffect(() => {
    if (notification && notification.type !== 'loading') {
      const timer = setTimeout(() => setNotification(null), 3000);
      return () => clearTimeout(timer);
    }
  }, [notification]);

  // Close editing when sidebar collapses
  useEffect(() => {
    if (!sidebarHovered && editingSession) {
      setEditingSession(null);
    }
  }, [sidebarHovered, editingSession]);

  const handleAddInstance = () => {
    const sessionNumber = instances.length + 1;
    const newInstance = {
      id: crypto.randomUUID ? crypto.randomUUID() : `session_${Date.now()}`,
      name: `Sesión ${sessionNumber}`,
      avatar: DEFAULT_AVATAR,
      unread: 0,
      isReady: false
    };
    setInstances([...instances, newInstance]);
    setActiveTab(newInstance.id);
    setNotification({ message: 'Iniciando entorno seguro...', type: 'loading' });
  };

  const handleSessionReady = (sessionId) => {
    setInstances(prev => {
      const session = prev.find(s => s.id === sessionId);
      if (!session || session.isReady) return prev;
      const updated = prev.map(s => s.id === sessionId ? { ...s, isReady: true } : s);
      setNotification({ message: 'Instancia creada exitosamente', type: 'success' });
      return updated;
    });
  };

  const handleUpdateSession = (sessionId, updates) => {
    setInstances(prev => prev.map(s => s.id === sessionId ? { ...s, ...updates } : s));
  };

  const handleSessionLoadFail = (sessionId, err) => {
    console.error(`Session ${sessionId} load fail:`, err);
    setInstances(prev => prev.filter(s => s.id !== sessionId));
    setNotification({ message: 'Error al cargar la instancia. Intenta de nuevo.', type: 'error' });
    if (activeTab === sessionId) {
      const remaining = instances.filter(s => s.id !== sessionId && s.isReady);
      setActiveTab(remaining.length ? remaining[0].id : 'settings');
    }
  };

  const confirmDeleteInstance = () => {
    const id = deleteModal;
    const newInst = instances.filter(i => i.id !== id);
    setInstances(newInst);
    if (activeTab === id) {
      const ready = newInst.filter(i => i.isReady);
      setActiveTab(ready.length ? ready[0].id : 'settings');
    }
    setDeleteModal(null);
    setNotification({ message: 'Instancia eliminada correctamente', type: 'success' });
  };

  const handleToggleSetting = async (name, value) => {
    switch (name) {
      case 'startWithWindows':
        setStartWithWindows(value);
        localStorage.setItem('startWithWindows', value);
        if (window.electron) await window.electron.toggleStartup(value);
        break;
      case 'startMinimized':
        setStartMinimized(value);
        localStorage.setItem('startMinimized', value);
        break;
      case 'closeToTray':
        setCloseToTray(value);
        localStorage.setItem('closeToTray', value);
        if (window.electron) await window.electron.setCloseToTray(value);
        break;
    }
  };

  const startEditingName = (session) => {
    setEditingSession(session.id);
    setEditingName(session.name);
  };

  const saveEditingName = () => {
    if (editingName.trim()) {
      handleUpdateSession(editingSession, { name: editingName.trim() });
    }
    setEditingSession(null);
    setEditingName('');
  };

  const cancelEditingName = () => {
    setEditingSession(null);
    setEditingName('');
  };

  const instanceToDelete = deleteModal ? instances.find(i => i.id === deleteModal) : null;

  const getTabIndex = (id) => (id === 'settings' ? -1 : instances.findIndex(i => i.id === id));
  const currentIndex = getTabIndex(activeTab);
  const previousIndex = getTabIndex(previousTabRef.current);
  const direction = currentIndex > previousIndex ? 1 : -1;
  useEffect(() => { previousTabRef.current = activeTab; }, [activeTab]);

  const getWebviewVariants = (id) => {
    const idx = instances.findIndex(i => i.id === id);
    const isActive = activeTab === id;
    if (isActive) return { y: 0, opacity: 1, zIndex: 10, transition: { type: 'spring', stiffness: 300, damping: 30 } };
    return idx < currentIndex
      ? { y: '-100%', opacity: 0, zIndex: 0, transition: { duration: 0.4, ease: 'easeInOut' } }
      : { y: '100%', opacity: 0, zIndex: 0, transition: { duration: 0.4, ease: 'easeInOut' } };
  };

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-[#0a0a0a] text-white font-sans">
      {/* Sidebar */}
      <motion.aside
        onMouseEnter={() => setSidebarHovered(true)}
        onMouseLeave={() => setSidebarHovered(false)}
        className="fixed left-0 top-0 h-full w-20 hover:w-80 bg-[#0a0a0a] z-[9999] transition-all duration-300 group border-r border-white/5 overflow-hidden flex flex-col py-6"
      >
        {/* Branding */}
        <div className="relative w-full h-16 flex items-center mb-8 cursor-pointer">
          <div className="absolute left-0 w-20 h-16 flex items-center justify-center z-20">
            <motion.div className="p-2.5 rounded-lg bg-supp-accent text-black shadow-lg shadow-supp-accent/20 cursor-pointer"
              whileHover={{ scale: 1.1, rotate: 12 }} whileTap={{ scale: 0.95 }}>
              <MessageSquare size={22} strokeWidth={2.5} />
            </motion.div>
          </div>
          <div className="absolute left-20 top-0 h-full flex flex-col justify-center z-10 opacity-0 group-hover:opacity-100 transition-opacity duration-300 delay-75 whitespace-nowrap">
            <h1 className="text-lg font-bold tracking-tight text-white leading-tight">Supp</h1>
            <span className="text-xs text-gray-500 font-medium leading-tight">by Javier Medina</span>
          </div>
        </div>

        {/* Sessions list */}
        <nav className="flex-1 flex flex-col gap-1 px-0 overflow-y-auto overflow-x-hidden scrollbar-thin scrollbar-thumb-gray-700 scrollbar-track-transparent max-h-[calc(100vh-300px)]">
          <AnimatePresence mode="popLayout">
            {instances.filter(i => i.isReady).map(instance => (
              <motion.button key={instance.id}
                layout
                initial={{ opacity: 0, x: -50 }}
                animate={{ opacity: 1, x: 0 }}
                exit={{ opacity: 0, scale: 0, transition: { duration: 0.2 } }}
                whileHover={{ scale: 1.02, backgroundColor: "rgba(255,255,255,0.05)" }}
                whileTap={{ scale: 0.95 }}
                onClick={() => setActiveTab(instance.id)}
                className={`relative w-full h-16 flex items-center cursor-pointer transition-colors outline-none ${activeTab === instance.id ? 'bg-supp-accent/10' : ''}`}
              >
                {/* Avatar */}
                <div className="absolute left-0 w-20 h-16 flex items-center justify-center z-20">
                  <motion.div className={`relative w-12 h-12 rounded-full transition-all duration-300 ${activeTab === instance.id ? 'ring-2 ring-supp-accent shadow-[0_0_15px_rgba(57,255,20,0.4)]' : 'opacity-60'} ${instance.unread > 0 && activeTab !== instance.id ? 'ring-2 ring-supp-accent/50 animate-pulse shadow-[0_0_20px_rgba(57,255,20,0.8)]' : ''}`}
                    whileHover={{ scale: 1.1 }} whileTap={{ scale: 0.95 }}>
                    <img src={instance.avatar} alt={instance.name} className="w-full h-full rounded-full object-cover bg-gray-800" />
                  </motion.div>
                </div>

                {/* Text */}
                <div className="absolute left-20 top-0 h-full flex flex-col justify-center z-10 opacity-0 group-hover:opacity-100 transition-opacity duration-300 delay-75 whitespace-nowrap pr-4 w-[calc(100%-80px)]">
                  {editingSession === instance.id ? (
                    <input type="text" value={editingName}
                      onChange={e => setEditingName(e.target.value)}
                      onKeyDown={e => { if (e.key === 'Enter') saveEditingName(); if (e.key === 'Escape') cancelEditingName(); }}
                      onBlur={saveEditingName}
                      className="h-8 bg-[#222] border border-white/20 text-white text-sm px-3 py-1 rounded-lg outline-none focus:border-supp-accent w-full shadow-lg z-50"
                      autoFocus />
                  ) : (
                    <div className="flex items-center justify-between w-full">
                      <div className="flex flex-col overflow-hidden">
                        <span className="text-sm font-semibold text-white truncate leading-tight">{instance.name}</span>
                        <span className="text-xs text-gray-500 truncate">WhatsApp Web</span>
                      </div>

                      <div className="flex items-center gap-2 ml-2">
                        {instance.unread > 0 && (
                          <motion.div initial={{ scale: 0 }} animate={{ scale: 1 }} className="min-w-[20px] h-5 px-1.5 rounded-full bg-supp-accent text-black text-xs font-bold flex items-center justify-center">
                            {instance.unread > 99 ? '99+' : instance.unread}
                          </motion.div>
                        )}
                        <div className="flex items-center opacity-0 group-hover:opacity-100 transition-opacity duration-200">
                          <motion.button whileHover={{ scale: 1.1 }} whileTap={{ scale: 0.95 }} onClick={e => { e.stopPropagation(); startEditingName(instance); }} className="p-1.5 rounded hover:bg-white/10 text-gray-400 hover:text-white transition-colors cursor-pointer">
                            <Pencil size={14} />
                          </motion.button>
                          <motion.button whileHover={{ scale: 1.1 }} whileTap={{ scale: 0.95 }} onClick={e => { e.stopPropagation(); setDeleteModal(instance.id); }} className="p-1.5 rounded hover:bg-red-500/20 text-gray-500 hover:text-red-400 transition-colors cursor-pointer">
                            <Trash2 size={16} />
                          </motion.button>
                        </div>
                      </div>
                    </div>
                  )}
                </div>
              </motion.button>
            ))}
          </AnimatePresence>

          {/* Add new session */}
          <motion.button
            layout
            onClick={handleAddInstance}
            whileHover={{ scale: 1.02, backgroundColor: "rgba(255,255,255,0.05)" }}
            whileTap={{ scale: 0.95 }}
            className="relative w-full h-16 flex items-center cursor-pointer group transition-colors outline-none mt-2"
          >
            <div className="absolute left-0 w-20 h-16 flex items-center justify-center z-20">
              <motion.div
                whileHover={{ scale: 1.1 }}
                whileTap={{ scale: 0.95 }}
                className="w-12 h-12 rounded-full border-2 border-dashed border-gray-600 flex items-center justify-center text-gray-500 group-hover:text-gray-300 group-hover:border-gray-400 transition-colors"
              >
                <Plus size={24} />
              </motion.div>
            </div>
            <div className="absolute left-20 top-0 h-full flex items-center z-10 opacity-0 group-hover:opacity-100 transition-opacity duration-300 delay-100 whitespace-nowrap">
              <span className="text-sm font-semibold text-gray-400 group-hover:text-white transition-colors duration-200">Nueva Sesión</span>
            </div>
          </motion.button>
        </nav>

        {/* Settings button */}
        <div className="mt-auto pt-4 pb-2">
          <motion.button
            onClick={() => setActiveTab('settings')}
            whileHover={{ scale: 1.02 }}
            whileTap={{ scale: 0.95 }}
            className={`relative w-full h-16 flex items-center cursor-pointer transition-colors outline-none ${activeTab === 'settings' ? 'bg-supp-accent/10' : 'hover:bg-white/5'}`}
          >
            <div className="absolute left-0 w-20 h-16 flex items-center justify-center z-20">
              <div className={`w-12 h-12 flex items-center justify-center rounded-xl transition-colors ${activeTab === 'settings' ? 'text-supp-accent' : 'text-gray-500 group-hover:text-white'}`}>
                <Settings size={24} />
              </div>
            </div>
            <div className="absolute left-20 top-0 h-full flex flex-col justify-center z-10 opacity-0 group-hover:opacity-100 transition-opacity duration-300 delay-75 whitespace-nowrap">
              <span className={`text-sm font-medium ${activeTab === 'settings' ? 'text-supp-accent' : 'text-gray-400 group-hover:text-white'}`}>Preferencias</span>
            </div>
          </motion.button>
        </div>
      </motion.aside>

      {/* Main content */}
      <main className="ml-20 flex-1 h-full relative overflow-hidden">
        {/* Settings view */}
        <AnimatePresence mode="wait">
          {activeTab === 'settings' && (
            <PreferencesPanel
              settings={{
                openAtLogin: startWithWindows,
                startMinimized: startMinimized,
                closeToTray: closeToTray
              }}
              toggleSetting={handleToggleSetting}
            />
          )}
        </AnimatePresence>

        {/* Webviews */}
        {instances.map(instance => (
          <SessionWebview key={instance.id} session={instance} isActive={activeTab === instance.id} onReady={handleSessionReady} onUpdateSession={handleUpdateSession} onLoadFail={handleSessionLoadFail} animationVariants={getWebviewVariants(instance.id)} />
        ))}
      </main>

      {/* Delete Modal */}
      <AnimatePresence>
        {deleteModal && (
          <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.2 }} className="fixed inset-0 z-[200] flex items-center justify-center bg-black/80 backdrop-blur-sm">
            <motion.div initial={{ scale: 0.9, opacity: 0 }} animate={{ scale: 1, opacity: 1 }} exit={{ scale: 0.9, opacity: 0 }} transition={{ type: 'spring', stiffness: 300, damping: 25 }} className="bg-[#1a1a1a] border border-white/10 rounded-2xl p-8 max-w-md w-full mx-4 shadow-2xl">
              <motion.div initial={{ scale: 0 }} animate={{ scale: 1 }} transition={{ delay: 0.1, type: 'spring', stiffness: 500, damping: 15 }} className="flex justify-center mb-6"><div className="p-4 rounded-full bg-red-500/10"><AlertTriangle size={48} className="text-red-400" /></div></motion.div>
              <motion.h2 initial={{ y: 10, opacity: 0 }} animate={{ y: 0, opacity: 1 }} transition={{ delay: 0.15 }} className="text-2xl font-bold text-white text-center mb-3">¿Eliminar instancia?</motion.h2>
              <motion.div initial={{ y: 10, opacity: 0 }} animate={{ y: 0, opacity: 1 }} transition={{ delay: 0.2 }}><p className="text-gray-400 text-center mb-2">Se eliminará la sesión <span className="text-white font-semibold">"{instanceToDelete?.name}"</span></p><p className="text-gray-500 text-sm text-center mb-8">Se perderán los datos locales de esta sesión de WhatsApp Web.</p></motion.div>
              <motion.div initial={{ y: 10, opacity: 0 }} animate={{ y: 0, opacity: 1 }} transition={{ delay: 0.25 }} className="flex gap-3"><motion.button whileHover={{ scale: 1.02 }} whileTap={{ scale: 0.98 }} onClick={() => setDeleteModal(null)} className="flex-1 px-6 py-3 rounded-xl bg-white/5 hover:bg-white/10 text-white font-medium transition-all duration-200 border border-white/10 cursor-pointer">Cancelar</motion.button><motion.button whileHover={{ scale: 1.02 }} whileTap={{ scale: 0.98 }} onClick={confirmDeleteInstance} className="flex-1 px-6 py-3 rounded-xl bg-red-500 hover:bg-red-600 text-white font-semibold transition-all duration-200 shadow-lg shadow-red-500/20 cursor-pointer">Eliminar</motion.button></motion.div>
            </motion.div>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Toast */}
      <AnimatePresence>
        {notification && (
          <motion.div initial={{ y: 50, opacity: 0 }} animate={{ y: 0, opacity: 1 }} exit={{ y: 50, opacity: 0 }} transition={{ type: 'spring', stiffness: 500, damping: 30 }} className="fixed bottom-8 left-1/2 -translate-x-1/2 z-[300]">
            <div className={`bg-[#1a1a1a] border-2 rounded-xl px-6 py-4 shadow-2xl flex items-center gap-3 ${notification.type === 'loading' ? 'border-yellow-500/50 shadow-yellow-500/10' : ''} ${notification.type === 'success' ? 'border-supp-accent shadow-supp-accent/20' : ''} ${notification.type === 'error' ? 'border-red-500 shadow-red-500/20' : ''}`}>
              {notification.type === 'loading' && (
                <motion.div animate={{ rotate: 360 }} transition={{ duration: 1, repeat: Infinity, ease: 'linear' }} className="p-1 rounded-full bg-yellow-500/20"><svg className="w-5 h-5 text-yellow-500" fill="none" viewBox="0 0 24 24"><circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle><path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path></svg></motion.div>
              )}
              {notification.type === 'success' && (
                <motion.div initial={{ scale: 0 }} animate={{ scale: 1 }} transition={{ delay: 0.1, type: 'spring', stiffness: 500, damping: 15 }} className="p-1 rounded-full bg-supp-accent/20"><CheckCircle size={20} className="text-supp-accent" /></motion.div>
              )}
              {notification.type === 'error' && (
                <motion.div initial={{ scale: 0 }} animate={{ scale: 1 }} transition={{ delay: 0.1, type: 'spring', stiffness: 500, damping: 15 }} className="p-1 rounded-full bg-red-500/20"><AlertTriangle size={20} className="text-red-500" /></motion.div>
              )}
              <p className="text-white font-medium">{notification.message}</p>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

export default App;
