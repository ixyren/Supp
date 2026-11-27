import React, { useState, useEffect } from 'react';
import { motion as Motion } from 'framer-motion';
import {
  Settings,
  Power,
  EyeOff,
  Inbox,
  PanelLeftOpen,
  Plus,
  MessageSquare,
  Pencil,
  Trash2,
  RefreshCw,
  ArrowUpRight,
  Keyboard,
  Image as ImageIcon,
  ExternalLink,
  GripVertical,
  Check,
  X,
  ShieldCheck,
  Download,
  CheckCircle2,
  AlertTriangle,
  Sparkles,
} from 'lucide-react';
import { Bridge } from '../bridge';
import { SPRING_SOFT, SPRING_SNAPPY, EASE_EXPAND } from './motionTokens';
import { MaterialSwitch, SessionAvatar } from './UiPrimitives';

function formatBytesMb(bytes) {
  if (!bytes || bytes <= 0) return '';
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function PreferencesPanel({
  settings,
  instances,
  isAddingSession,
  canReloadSession,
  toggleSetting,
  onAddInstance,
  onSelectSession,
  onRenameSession,
  onReloadSession,
  onDeleteSession,
  onNotify,
}) {
  const [inlineEditId, setInlineEditId] = useState(null);
  const [inlineEditValue, setInlineEditValue] = useState('');
  const [updateStatus, setUpdateStatus] = useState('idle'); // idle | checking | up_to_date | available | downloading | error
  const [updateInfo, setUpdateInfo] = useState(null);
  const [downloadProgress, setDownloadProgress] = useState({ downloaded: 0, total: 0, percent: 0 });
  const [updateError, setUpdateError] = useState('');

  useEffect(() => {
    let cancelled = false;
    let unlisten = null;

    Bridge.listenUpdateProgress((payload) => {
      if (!cancelled) {
        setDownloadProgress(payload);
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

  const handleCheckUpdates = async () => {
    if (updateStatus === 'checking' || updateStatus === 'downloading') return;
    setUpdateStatus('checking');
    setUpdateError('');
    try {
      const info = await Bridge.checkForUpdates();
      setUpdateInfo(info);
      if (info && info.hasUpdate) {
        setUpdateStatus('available');
        onNotify?.({
          message: `Nueva versión v${info.latestVersion} disponible`,
          type: 'success',
        });
      } else {
        setUpdateStatus('up_to_date');
        onNotify?.({
          message: `Supp ya está actualizado (v${info?.currentVersion || '1.0.5'})`,
          type: 'success',
        });
      }
    } catch (err) {
      const msg = typeof err === 'string' ? err : err?.message || 'Error conectando con GitHub';
      setUpdateError(msg);
      setUpdateStatus('error');
      onNotify?.({ message: 'No se pudo comprobar actualizaciones', type: 'error' });
    }
  };

  const handleInstallUpdate = async () => {
    if (!updateInfo || !updateInfo.downloadUrl || updateStatus === 'downloading') return;
    setUpdateStatus('downloading');
    setUpdateError('');
    setDownloadProgress({ downloaded: 0, total: updateInfo.assetSize || 0, percent: 0 });
    onNotify?.({ message: 'Descargando actualización de Supp...', type: 'loading' });

    try {
      await Bridge.installUpdate({
        downloadUrl: updateInfo.downloadUrl,
        assetName: updateInfo.assetName,
        isPortable: updateInfo.isPortable,
      });
    } catch (err) {
      const msg = typeof err === 'string' ? err : err?.message || 'No se pudo instalar la actualización';
      setUpdateError(msg);
      setUpdateStatus('error');
      onNotify?.({ message: 'Error descargando o instalando actualización', type: 'error' });
    }
  };

  const totalUnread = instances.reduce((acc, inst) => acc + (inst.unread || 0), 0);

  const startInlineRename = (inst) => {
    setInlineEditId(inst.id);
    setInlineEditValue(inst.name);
  };

  const commitInlineRename = () => {
    if (inlineEditId && inlineEditValue.trim()) {
      onRenameSession(inlineEditId, inlineEditValue.trim());
    }
    setInlineEditId(null);
    setInlineEditValue('');
  };

  const cancelInlineRename = () => {
    setInlineEditId(null);
    setInlineEditValue('');
  };

  const toggleItems = [
    {
      key: 'startWithWindows',
      active: settings.openAtLogin,
      icon: Power,
      title: 'Inicio con Windows',
      description: 'Inicia Supp automáticamente en segundo plano al encender tu equipo.',
    },
    {
      key: 'startMinimized',
      active: settings.startMinimized,
      icon: EyeOff,
      title: 'Iniciar minimizado',
      description: 'Abre directamente en la bandeja del sistema sin interrumpir tus ventanas.',
    },
    {
      key: 'closeToTray',
      active: settings.closeToTray,
      icon: Inbox,
      title: 'Minimizar a la bandeja',
      description: 'Al cerrar la ventana (X), las sesiones continúan activas en segundo plano.',
    },
    {
      key: 'hoverExpand',
      active: settings.hoverExpandEnabled,
      icon: PanelLeftOpen,
      title: 'Despliegue al pasar el mouse',
      description: 'Expande automáticamente el panel lateral al acercar el cursor al borde izquierdo.',
    },
  ];

  const tips = [
    {
      icon: Keyboard,
      kbd: 'Ctrl + R',
      title: 'Recarga instantánea',
      desc: 'Refresca la sesión activa si WhatsApp Web pierde conexión o tarda en cargar.',
    },
    {
      icon: ImageIcon,
      kbd: 'Clic derecho',
      title: 'Copiar imágenes y videos',
      desc: 'Copia imágenes, fotogramas o archivos de video reales directo al portapapeles.',
    },
    {
      icon: ExternalLink,
      kbd: 'Enlaces',
      title: 'Navegador predeterminado',
      desc: 'Cualquier enlace externo se abre fuera de tus sesiones para mayor seguridad.',
    },
    {
      icon: GripVertical,
      kbd: 'Arrastrar',
      title: 'Orden personalizado',
      desc: 'Reorganiza tus cuentas arrastrando el indicador en la barra lateral.',
    },
  ];

  return (
    <div className="absolute inset-0 z-10 bg-[#12150e] text-[#e7ebdc] select-none overflow-y-auto scrollbar-thin">
      {/* Static ambient radial surface accent (zero blur filter) */}
      <div className="absolute inset-0 bg-[radial-gradient(ellipse_75%_50%_at_58%_0%,rgba(198,255,0,0.065)_0%,transparent_72%)] pointer-events-none" />

      <div className="relative z-10 max-w-5xl mx-auto px-6 sm:px-8 py-8 flex flex-col gap-5">
        {/* Bento Row 1: Hero Header Card */}
        <section
          className="bg-[#181c13] border border-[#2c3323] rounded-[28px] p-6 m3-elevation-2 relative overflow-hidden"
        >
          <div className="absolute right-0 top-0 w-72 h-44 bg-[radial-gradient(circle_at_top_right,rgba(198,255,0,0.11),transparent_70%)] pointer-events-none" />

          <div className="relative flex flex-col md:flex-row md:items-center justify-between gap-5">
            <div className="flex items-start sm:items-center gap-4">
              <Motion.div
                whileHover={{ rotate: 20, scale: 1.06 }}
                transition={SPRING_SNAPPY}
                style={{ willChange: 'transform' }}
                className="w-14 h-14 rounded-[20px] bg-[#2a3e00] border border-[#c6ff00]/30 text-[#c6ff00] flex items-center justify-center shrink-0 shadow-[0_6px_22px_rgba(198,255,0,0.14)]"
              >
                <Settings size={28} strokeWidth={2.2} />
              </Motion.div>

              <div>
                <div className="flex flex-wrap items-center gap-2 mb-1">
                  <h2 className="text-2xl font-extrabold tracking-tight text-[#e7ebdc]">
                    Centro de Control
                  </h2>
                  <span className="text-[10px] font-bold tracking-wider uppercase px-2.5 py-0.5 rounded-full bg-[#21271a] text-[#c6ff00] border border-[#c6ff00]/25">
                    Material You 3
                  </span>
                </div>
                <p className="text-xs sm:text-sm text-[#a2a895]">
                  Configura el comportamiento de Supp y administra tus sesiones de WhatsApp Web
                </p>
              </div>
            </div>

            {/* Telemetry Chips + Quick Add CTA */}
            <div className="flex flex-wrap items-center gap-2.5 shrink-0">
              <div className="px-3 py-1.5 rounded-2xl bg-[#21271a] border border-[#2c3323] flex items-center gap-2 text-xs">
                <span className="w-2 h-2 rounded-full bg-[#c6ff00]" />
                <span className="font-bold text-[#e7ebdc] tabular-nums">{instances.length}</span>
                <span className="text-[#a2a895]">
                  {instances.length === 1 ? 'sesión' : 'sesiones'}
                </span>
              </div>

              {totalUnread > 0 && (
                <div className="px-3 py-1.5 rounded-2xl bg-[#2a3e00]/80 border border-[#c6ff00]/30 flex items-center gap-1.5 text-xs">
                  <span className="font-black text-[#c6ff00] tabular-nums">{totalUnread}</span>
                  <span className="text-[#d8ff70] font-medium">sin leer</span>
                </div>
              )}

              <Motion.button
                type="button"
                disabled={isAddingSession}
                onClick={onAddInstance}
                whileHover={{ scale: 1.03 }}
                whileTap={{ scale: 0.96 }}
                transition={SPRING_SNAPPY}
                className="h-9 px-4 rounded-2xl bg-[#c6ff00] hover:bg-[#d8ff59] text-[#182900] text-xs font-bold flex items-center gap-1.5 shadow-[0_4px_16px_rgba(198,255,0,0.22)] cursor-pointer disabled:opacity-60"
              >
                {isAddingSession ? (
                  <Motion.span
                    animate={{ rotate: 360 }}
                    transition={{ duration: 0.9, repeat: Infinity, ease: 'linear' }}
                    className="flex items-center"
                  >
                    <RefreshCw size={14} strokeWidth={2.5} />
                  </Motion.span>
                ) : (
                  <Plus size={15} strokeWidth={2.6} />
                )}
                <span>{isAddingSession ? 'Creando...' : 'Nueva sesión'}</span>
              </Motion.button>
            </div>
          </div>
        </section>

        {/* Bento Row 2: Active Sessions Quick-Overview OR Inviting Empty State */}
        {instances.length === 0 ? (
          <section
            className="bg-[#181c13] border border-[#c6ff00]/25 rounded-[28px] p-7 m3-elevation-2 relative overflow-hidden"
          >
            <div className="absolute inset-0 bg-[radial-gradient(ellipse_at_center,rgba(198,255,0,0.07),transparent_68%)] pointer-events-none" />
            <div className="relative flex flex-col items-center text-center max-w-lg mx-auto py-3">
              <div className="w-16 h-16 rounded-[22px] bg-[#2a3e00] border border-[#c6ff00]/35 text-[#c6ff00] flex items-center justify-center mb-4 shadow-[0_0_28px_rgba(198,255,0,0.2)]">
                <MessageSquare size={30} strokeWidth={2.3} />
              </div>

              <h3 className="text-xl font-bold text-[#e7ebdc] mb-2">
                Aún no tienes sesiones activas
              </h3>
              <p className="text-xs sm:text-sm text-[#a2a895] leading-relaxed mb-5">
                Crea tu primera sesión para conectar cuentas personales, de trabajo o soporte en entornos aislados con motor nativo WebView2.
              </p>

              <div className="flex flex-wrap items-center justify-center gap-2 mb-6">
                <span className="inline-flex items-center gap-1.5 text-[11px] font-medium px-3 py-1 rounded-full bg-[#21271a] border border-[#2c3323] text-[#e7ebdc]">
                  <ShieldCheck size={13} className="text-[#c6ff00]" />
                  Caché independiente por cuenta
                </span>
                <span className="inline-flex items-center gap-1.5 text-[11px] font-medium px-3 py-1 rounded-full bg-[#21271a] border border-[#2c3323] text-[#e7ebdc]">
                  <ImageIcon size={13} className="text-[#c6ff00]" />
                  Copia imágenes y videos con clic derecho
                </span>
              </div>

              <Motion.button
                type="button"
                disabled={isAddingSession}
                onClick={onAddInstance}
                whileHover={{ scale: 1.03 }}
                whileTap={{ scale: 0.96 }}
                transition={SPRING_SNAPPY}
                className="h-11 px-6 rounded-2xl bg-[#c6ff00] hover:bg-[#d8ff59] text-[#182900] text-sm font-extrabold flex items-center gap-2 shadow-[0_6px_24px_rgba(198,255,0,0.28)] cursor-pointer disabled:opacity-60"
              >
                <Plus size={18} strokeWidth={2.6} />
                <span>{isAddingSession ? 'Creando sesión...' : 'Crear mi primera sesión'}</span>
              </Motion.button>
            </div>
          </section>
        ) : (
          <section
            className="bg-[#181c13] border border-[#2c3323] rounded-[28px] p-5 m3-elevation-1"
          >
            <div className="flex items-center justify-between mb-3.5 px-1">
              <div>
                <h3 className="text-sm font-bold text-[#e7ebdc] tracking-tight">
                  Sesiones activas
                </h3>
                <p className="text-[11px] text-[#a2a895]">
                  Cambia, renombra o administra tus cuentas desde aquí
                </p>
              </div>
              <span className="text-[11px] font-semibold text-[#737a67] tabular-nums">
                {instances.length} {instances.length === 1 ? 'cuenta' : 'cuentas'}
              </span>
            </div>

            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
              {instances.map((inst) => {
                const isEditingHere = inlineEditId === inst.id;
                return (
                  <Motion.div
                    key={inst.id}
                    whileHover={!isEditingHere ? { y: -2 } : undefined}
                    transition={SPRING_SOFT}
                    onClick={() => {
                      if (!isEditingHere) onSelectSession(inst.id);
                    }}
                    className="group bg-[#21271a] hover:bg-[#262d1e] border border-[#2c3323] hover:border-[#c6ff00]/35 rounded-2xl p-3.5 flex flex-col gap-3 cursor-pointer transition-colors"
                  >
                    <div className="flex items-center gap-3 min-w-0">
                      <div className="relative w-10 h-10 rounded-full shrink-0 ring-1 ring-[#474e3b] group-hover:ring-[#c6ff00]/60 transition-all">
                        <SessionAvatar
                          src={inst.avatar}
                          alt={inst.name}
                          className="w-full h-full rounded-full object-cover bg-[#181c13]"
                        />
                        <span
                          className={`absolute -bottom-0.5 -right-0.5 w-3 h-3 rounded-full border-2 border-[#21271a] ${
                            inst.hasError ? 'bg-red-400' : 'bg-[#c6ff00]'
                          }`}
                        />
                      </div>

                      <div className="flex-1 min-w-0">
                        {isEditingHere ? (
                          <div
                            className="flex items-center gap-1"
                            onClick={(e) => e.stopPropagation()}
                          >
                            <input
                              type="text"
                              autoFocus
                              onFocus={(e) => e.target.select()}
                              maxLength={32}
                              value={inlineEditValue}
                              onChange={(e) => setInlineEditValue(e.target.value)}
                              onKeyDown={(e) => {
                                if (e.key === 'Enter') commitInlineRename();
                                if (e.key === 'Escape') cancelInlineRename();
                              }}
                              className="h-7 w-full min-w-0 bg-[#12150e] border border-[#c6ff00]/65 rounded-lg px-2 text-xs text-[#e7ebdc] outline-none"
                            />
                            <button
                              type="button"
                              onClick={commitInlineRename}
                              className="p-1 rounded-lg bg-[#c6ff00] text-[#182900] cursor-pointer"
                              aria-label="Guardar"
                            >
                              <Check size={13} strokeWidth={2.6} />
                            </button>
                            <button
                              type="button"
                              onClick={cancelInlineRename}
                              className="p-1 rounded-lg bg-[#181c13] text-[#a2a895] hover:text-[#e7ebdc] cursor-pointer"
                              aria-label="Cancelar"
                            >
                              <X size={13} />
                            </button>
                          </div>
                        ) : (
                          <>
                            <div className="flex items-center gap-1.5">
                              <span className="text-sm font-bold text-[#e7ebdc] group-hover:text-[#c6ff00] transition-colors truncate">
                                {inst.name}
                              </span>
                              {inst.unread > 0 && (
                                <span className="px-1.5 py-0.5 rounded-full bg-[#c6ff00] text-[#182900] text-[10px] font-black tabular-nums shrink-0">
                                  {inst.unread > 99 ? '99+' : inst.unread}
                                </span>
                              )}
                            </div>
                            <p className="text-[11px] text-[#a2a895] truncate">
                              {inst.hasError
                                ? 'Error de conexión'
                                : inst.unread > 0
                                ? `${inst.unread} mensajes sin leer`
                                : 'Lista en segundo plano'}
                            </p>
                          </>
                        )}
                      </div>
                    </div>

                    {/* Quick Action Bar */}
                    <div
                      className="flex items-center justify-between pt-2 border-t border-[#2c3323]/80"
                      onClick={(e) => e.stopPropagation()}
                    >
                      <button
                        type="button"
                        onClick={() => onSelectSession(inst.id)}
                        className="inline-flex items-center gap-1 text-[11px] font-bold text-[#c6ff00] hover:text-[#d8ff59] px-2 py-1 rounded-lg hover:bg-[#2a3e00]/60 transition-colors cursor-pointer"
                      >
                        <span>Abrir</span>
                        <ArrowUpRight size={13} />
                      </button>

                      <div className="flex items-center gap-1">
                        <button
                          type="button"
                          onClick={() => startInlineRename(inst)}
                          className="p-1.5 rounded-lg text-[#a2a895] hover:text-[#c6ff00] hover:bg-[#181c13] transition-colors cursor-pointer"
                          title="Renombrar sesión"
                        >
                          <Pencil size={13} />
                        </button>

                        {canReloadSession && (
                          <button
                            type="button"
                            onClick={() => onReloadSession(inst.id)}
                            className="p-1.5 rounded-lg text-[#a2a895] hover:text-[#c6ff00] hover:bg-[#181c13] transition-colors cursor-pointer"
                            title="Recargar sesión"
                          >
                            <RefreshCw size={13} />
                          </button>
                        )}

                        <button
                          type="button"
                          onClick={() => onDeleteSession(inst.id)}
                          className="p-1.5 rounded-lg text-[#a2a895] hover:text-red-400 hover:bg-red-500/15 transition-colors cursor-pointer"
                          title="Eliminar sesión"
                        >
                          <Trash2 size={13} />
                        </button>
                      </div>
                    </div>
                  </Motion.div>
                );
              })}
            </div>
          </section>
        )}

        {/* Bento Row 3: System Behavior Toggles (2x2 Grid) */}
        <section
          className="grid grid-cols-1 md:grid-cols-2 gap-3.5"
        >
          {toggleItems.map((item) => {
            const Icon = item.icon;
            return (
              <Motion.div
                key={item.key}
                whileHover={{ scale: 1.01, y: -1 }}
                whileTap={{ scale: 0.99 }}
                transition={SPRING_SOFT}
                onClick={() => toggleSetting(item.key, !item.active)}
                className="bg-[#181c13] border border-[#2c3323] hover:border-[#474e3b] rounded-3xl p-4 sm:p-5 flex items-center justify-between gap-4 cursor-pointer transition-colors m3-elevation-1"
              >
                <div className="flex items-start gap-3.5 min-w-0">
                  <div
                    className={`w-10 h-10 rounded-2xl flex items-center justify-center shrink-0 border transition-colors duration-200 ${
                      item.active
                        ? 'bg-[#2a3e00] border-[#c6ff00]/30 text-[#c6ff00]'
                        : 'bg-[#21271a] border-[#2c3323] text-[#a2a895]'
                    }`}
                  >
                    <Icon size={18} strokeWidth={2.2} />
                  </div>
                  <div className="min-w-0">
                    <h3 className="font-semibold text-[#e7ebdc] text-sm sm:text-[15px] leading-snug">
                      {item.title}
                    </h3>
                    <p className="text-xs text-[#a2a895] mt-1 leading-relaxed">
                      {item.description}
                    </p>
                  </div>
                </div>

                <MaterialSwitch
                  active={item.active}
                  ariaLabel={item.title}
                  onClick={(e) => {
                    e.stopPropagation();
                    toggleSetting(item.key, !item.active);
                  }}
                />
              </Motion.div>
            );
          })}
        </section>

        {/* Bento Row 4: Shortcuts & Tips */}
        <section
          className="bg-[#181c13] border border-[#2c3323] rounded-[28px] p-5 m3-elevation-1"
        >
          <div className="flex items-center justify-between mb-3.5 px-1">
            <h3 className="text-sm font-bold text-[#e7ebdc] tracking-tight">
              Atajos y trucos rápidos
            </h3>
            <span className="text-[11px] text-[#737a67]">Productividad diaria</span>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
            {tips.map((tip) => {
              const Icon = tip.icon;
              return (
                <div
                  key={tip.title}
                  className="bg-[#21271a]/75 border border-[#2c3323] rounded-2xl p-3.5 flex flex-col gap-2"
                >
                  <div className="flex items-center justify-between gap-2">
                    <div className="w-8 h-8 rounded-xl bg-[#181c13] border border-[#2c3323] text-[#c6ff00] flex items-center justify-center shrink-0">
                      <Icon size={15} strokeWidth={2.2} />
                    </div>
                    <span className="text-[10px] font-bold tracking-wide px-2 py-0.5 rounded-full bg-[#2a3e00]/80 text-[#c6ff00] border border-[#c6ff00]/25">
                      {tip.kbd}
                    </span>
                  </div>
                  <h4 className="text-xs font-bold text-[#e7ebdc] mt-0.5">{tip.title}</h4>
                  <p className="text-[11px] text-[#a2a895] leading-relaxed">{tip.desc}</p>
                </div>
              );
            })}
          </div>
        </section>

        {/* Bento Row 5: Actualizaciones automáticas desde GitHub */}
        <section className="bg-[#181c13] border border-[#2c3323] rounded-[28px] p-5 m3-elevation-1">
          <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
            <div className="flex items-start gap-3.5 min-w-0">
              <div className="w-11 h-11 rounded-2xl bg-[#2a3e00] border border-[#c6ff00]/30 text-[#c6ff00] flex items-center justify-center shrink-0">
                <Sparkles size={20} strokeWidth={2.2} />
              </div>

              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2">
                  <h3 className="text-sm font-bold text-[#e7ebdc] tracking-tight">
                    Actualizaciones desde GitHub
                  </h3>
                  <span className="text-[10px] font-bold px-2 py-0.5 rounded-full bg-[#21271a] border border-[#2c3323] text-[#c6ff00] tabular-nums">
                    v{updateInfo?.currentVersion || '1.0.5'}
                  </span>
                  {updateInfo && (
                    <span className="text-[10px] font-semibold px-2 py-0.5 rounded-full bg-[#21271a] border border-[#2c3323] text-[#a2a895]">
                      {updateInfo.isPortable ? 'Modo Portable' : 'Instalado'}
                    </span>
                  )}
                </div>

                <p className="text-xs text-[#a2a895] mt-1">
                  {updateStatus === 'checking'
                    ? 'Consultando último lanzamiento en ixyren/Supp...'
                    : updateStatus === 'available' && updateInfo
                    ? `Nueva versión v${updateInfo.latestVersion} lista para instalar (${updateInfo.assetName || 'GitHub Release'}).`
                    : updateStatus === 'up_to_date' && updateInfo
                    ? `Tienes la última versión disponible (v${updateInfo.latestVersion}).`
                    : updateStatus === 'downloading'
                    ? `Descargando ${updateInfo?.assetName || 'actualización'} (${downloadProgress.percent}%)${
                        downloadProgress.total > 0
                          ? ` • ${formatBytesMb(downloadProgress.downloaded)} / ${formatBytesMb(downloadProgress.total)}`
                          : ''
                      }...`
                    : updateStatus === 'error'
                    ? updateError || 'No se pudo comprobar actualizaciones.'
                    : 'Comprueba e instala nuevas versiones del instalador o portable directamente desde GitHub Releases.'}
                </p>
              </div>
            </div>

            <div className="flex flex-wrap items-center gap-2 shrink-0">
              <button
                type="button"
                onClick={() =>
                  Bridge.openExternalUrl(
                    updateInfo?.releaseUrl || 'https://github.com/ixyren/Supp/releases'
                  )
                }
                className="h-9 px-3.5 rounded-2xl bg-[#21271a] hover:bg-[#2b3222] border border-[#2c3323] text-xs font-semibold text-[#e7ebdc] inline-flex items-center gap-1.5 transition-colors cursor-pointer"
              >
                <span>GitHub</span>
                <ExternalLink size={13} className="text-[#a2a895]" />
              </button>

              {updateStatus === 'available' && updateInfo?.downloadUrl ? (
                <Motion.button
                  type="button"
                  onClick={handleInstallUpdate}
                  whileHover={{ scale: 1.03 }}
                  whileTap={{ scale: 0.96 }}
                  transition={SPRING_SNAPPY}
                  className="h-9 px-4 rounded-2xl bg-[#c6ff00] hover:bg-[#d8ff59] text-[#182900] text-xs font-extrabold inline-flex items-center gap-1.5 shadow-[0_4px_16px_rgba(198,255,0,0.22)] cursor-pointer"
                >
                  <Download size={14} strokeWidth={2.5} />
                  <span>Actualizar a v{updateInfo.latestVersion}</span>
                </Motion.button>
              ) : (
                <Motion.button
                  type="button"
                  disabled={updateStatus === 'checking' || updateStatus === 'downloading'}
                  onClick={handleCheckUpdates}
                  whileHover={{ scale: 1.02 }}
                  whileTap={{ scale: 0.96 }}
                  transition={SPRING_SNAPPY}
                  className="h-9 px-4 rounded-2xl bg-[#2a3e00] hover:bg-[#354f00] border border-[#c6ff00]/35 text-[#c6ff00] text-xs font-bold inline-flex items-center gap-1.5 cursor-pointer disabled:opacity-60 transition-colors"
                >
                  {updateStatus === 'checking' || updateStatus === 'downloading' ? (
                    <Motion.span
                      animate={{ rotate: 360 }}
                      transition={{ duration: 0.9, repeat: Infinity, ease: 'linear' }}
                      className="flex items-center"
                    >
                      <RefreshCw size={13} strokeWidth={2.5} />
                    </Motion.span>
                  ) : updateStatus === 'up_to_date' ? (
                    <CheckCircle2 size={14} />
                  ) : updateStatus === 'error' ? (
                    <AlertTriangle size={14} className="text-red-400" />
                  ) : (
                    <RefreshCw size={13} />
                  )}
                  <span>
                    {updateStatus === 'checking'
                      ? 'Buscando...'
                      : updateStatus === 'downloading'
                      ? `Descargando ${downloadProgress.percent}%`
                      : 'Buscar actualizaciones'}
                  </span>
                </Motion.button>
              )}
            </div>
          </div>

          {updateStatus === 'downloading' && (
            <div className="mt-4 w-full h-2 rounded-full bg-[#12150e] border border-[#2c3323] overflow-hidden p-[1px]">
              <Motion.div
                initial={{ scaleX: 0 }}
                animate={{ scaleX: Math.max(0.02, Math.min(1, downloadProgress.percent / 100)) }}
                transition={{ duration: 0.16, ease: EASE_EXPAND }}
                style={{ transformOrigin: 'left center', willChange: 'transform' }}
                className="w-full h-full rounded-full bg-[#c6ff00]"
              />
            </div>
          )}

          {updateStatus === 'available' && updateInfo?.releaseNotes && (
            <div className="mt-3.5 p-3 rounded-2xl bg-[#12150e] border border-[#2c3323] text-[11px] text-[#a2a895] leading-relaxed max-h-24 overflow-y-auto scrollbar-thin whitespace-pre-wrap">
              {updateInfo.releaseNotes}
            </div>
          )}
        </section>

        {/* Footer Branding */}
        <div className="pt-1 pb-3 flex items-center justify-center gap-2 text-[#737a67] text-xs">
          <span className="w-1.5 h-1.5 rounded-full bg-[#c6ff00]" />
          <span>Supp Multi-Session</span>
          <span>•</span>
          <span>Rust + WebView2 Engine</span>
          <span>•</span>
          <span>Javier Medina</span>
        </div>
      </div>
    </div>
  );
}
