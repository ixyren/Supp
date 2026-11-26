import React, { useRef, useEffect } from 'react';
import {
  MessageSquare,
  Settings,
  Plus,
  Trash2,
  Check,
  Pencil,
  X,
  GripVertical,
  RefreshCw,
} from 'lucide-react';
import { motion as Motion, AnimatePresence, Reorder } from 'framer-motion';
import {
  EASE_EXPAND,
  EASE_EXIT,
  SPRING_SNAPPY,
  SPRING_LAYOUT,
} from './motionTokens';
import { RevealLabel, SessionAvatar } from './UiPrimitives';

export function SidebarRail({
  activeTab,
  instances,
  isSidebarExpanded,
  hoverExpandEnabled,
  editingSession,
  deletingSessionId,
  editingName,
  setEditingName,
  isAddingSession,
  onMouseEnter,
  onMouseLeave,
  onToggleHoverExpand,
  onTabChange,
  onReorder,
  onOpenContextMenu,
  onStartEditing,
  onSaveEditing,
  onCancelEditing,
  onRequestDelete,
  onAddInstance,
  onDragStateChange,
}) {
  const renameInputRef = useRef(null);

  useEffect(() => {
    if (editingSession && renameInputRef.current) {
      renameInputRef.current.focus();
      renameInputRef.current.select();
    }
  }, [editingSession]);

  return (
    <aside
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
      className={`${
        activeTab === 'settings'
          ? `${
              isSidebarExpanded
                ? 'w-[280px] duration-[220ms] ease-[cubic-bezier(0.16,1,0.3,1)]'
                : 'w-[80px] duration-[160ms] ease-[cubic-bezier(0.2,0.9,0.3,1)]'
            } transition-[width]`
          : 'w-full min-w-[80px] max-w-[280px]'
      } h-full bg-[#181c13] border-r border-[#2c3323] m3-elevation-3 overflow-hidden flex flex-col py-4 select-none shrink-0 relative`}
    >
      {/* Subtle top radial accent (static, zero blur) */}
      <div className="absolute inset-x-0 top-0 h-32 bg-[radial-gradient(ellipse_at_top_left,rgba(198,255,0,0.06),transparent_70%)] pointer-events-none" />

      {/* Brand Header: 10px + 60px + 196px + 9px = 275px inside 279px inner (or exact 60px inside 79px collapsed) */}
      <div className="relative w-full h-14 flex items-center pl-[10px] pr-[9px] shrink-0 z-10">
        <div className="w-[60px] h-14 flex items-center justify-center shrink-0 z-20">
          <Motion.button
            type="button"
            onClick={onToggleHoverExpand}
            whileHover={{ scale: 1.06, rotate: -3 }}
            whileTap={{ scale: 0.91, rotate: 0 }}
            transition={SPRING_SNAPPY}
            style={{ willChange: 'transform' }}
            title={
              hoverExpandEnabled
                ? 'Despliegue al pasar el mouse: Activado (Clic para desactivar)'
                : 'Despliegue al pasar el mouse: Desactivado (Clic para activar)'
            }
            className={`relative w-12 h-12 rounded-[18px] flex items-center justify-center cursor-pointer transition-colors duration-200 outline-none select-none ${
              hoverExpandEnabled
                ? 'bg-[#c6ff00] text-[#182900] shadow-[0_4px_18px_rgba(198,255,0,0.28)] hover:bg-[#d8ff59]'
                : 'bg-[#21271a] text-[#a2a895] border border-[#474e3b] hover:text-[#c6ff00] hover:border-[#c6ff00]/50 hover:bg-[#2b3222]'
            }`}
          >
            <MessageSquare size={22} strokeWidth={2.5} />
            <span
              className={`absolute -top-0.5 -right-0.5 w-3 h-3 rounded-full border-2 border-[#181c13] transition-colors duration-200 ${
                hoverExpandEnabled ? 'bg-[#c6ff00]' : 'bg-[#474e3b]'
              }`}
            />
          </Motion.button>
        </div>

        <RevealLabel
          show={isSidebarExpanded}
          className="w-[196px] shrink-0 flex items-center pl-2 pr-1 whitespace-nowrap overflow-hidden"
        >
          <div className="flex flex-col min-w-0 whitespace-nowrap">
            <div className="flex items-center gap-2">
              <h1 className="text-lg font-extrabold tracking-tight text-[#e7ebdc] leading-none">
                Supp
              </h1>
              <span className="text-[9px] font-bold tracking-wider uppercase px-2 py-0.5 rounded-full bg-[#2a3e00] text-[#c6ff00] border border-[#c6ff00]/30">
                Rust
              </span>
            </div>
            <span className="text-[11px] text-[#a2a895] font-medium mt-1 whitespace-nowrap">
              WhatsApp Multi-Sesión
            </span>
          </div>
        </RevealLabel>
      </div>

      {/* Navigation Group */}
      <div className="flex-1 flex flex-col min-h-0">
        {/* Sessions Navigation List */}
        <div
          className={`flex-1 flex flex-col pl-[10px] pr-[9px] pt-4 pb-2 overflow-y-auto overflow-x-hidden z-10 ${
            isSidebarExpanded ? 'scrollbar-rail-expanded' : 'scrollbar-rail-collapsed'
          }`}
        >
          <Reorder.Group
            axis="y"
            values={instances}
            onReorder={onReorder}
            className="flex flex-col gap-1.5 list-none p-0 m-0"
          >
            <AnimatePresence initial={false}>
              {instances.map((instance, idx) => {
                const isSelected = activeTab === instance.id;
                const isEditing = editingSession === instance.id;
                const dragAllowed =
                  isSidebarExpanded && !isEditing && !deletingSessionId;

                return (
                  <Reorder.Item
                    key={instance.id}
                    value={instance}
                    layout="position"
                    dragListener={dragAllowed}
                    onDragStart={() => onDragStateChange?.(true)}
                    onDragEnd={() => onDragStateChange?.(false)}
                    whileDrag={{
                      scale: 1.025,
                      boxShadow: '0 14px 32px rgba(0, 0, 0, 0.65)',
                      zIndex: 50,
                      cursor: 'grabbing',
                    }}
                    initial={{ opacity: 0, scale: 0.88, y: 14 }}
                    animate={{ opacity: 1, scale: 1, y: 0 }}
                    exit={{
                      opacity: 0,
                      scale: 0.82,
                      x: -28,
                      transition: { duration: 0.16, ease: EASE_EXIT },
                    }}
                    whileHover={!isEditing ? { scale: 1.012 } : undefined}
                    whileTap={!isEditing ? { scale: 0.97 } : undefined}
                    transition={SPRING_LAYOUT}
                    onClick={() => {
                      if (!isEditing) onTabChange(instance.id);
                    }}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      e.stopPropagation();
                      onOpenContextMenu(instance, e.clientX, e.clientY);
                    }}
                    title={
                      !isSidebarExpanded
                        ? `${instance.name}${instance.unread > 0 ? ` (${instance.unread} sin leer)` : ''} — Clic derecho para opciones`
                        : undefined
                    }
                    className={`group/session relative w-full h-[58px] rounded-2xl flex items-center cursor-pointer overflow-hidden outline-none list-none select-none transition-colors duration-150 ${
                      isSelected ? '' : 'hover:bg-[#21271a]'
                    }`}
                  >
                    {/* Hover border overlay (doesn't steal 2px from 60px collapsed icon column) */}
                    {!isSelected && (
                      <span className="absolute inset-0 rounded-2xl border border-transparent group-hover/session:border-[#2c3323] transition-colors pointer-events-none" />
                    )}

                    {/* Active Pill Indicator (GPU-composited opacity + scaleY accent, zero layout projection distortion) */}
                    <AnimatePresence initial={false}>
                      {isSelected && (
                        <Motion.div
                          key="active-session-pill"
                          initial={{ opacity: 0 }}
                          animate={{ opacity: 1 }}
                          exit={{ opacity: 0 }}
                          transition={{ duration: 0.16, ease: EASE_EXPAND }}
                          style={{ willChange: 'opacity' }}
                          className="absolute inset-0 rounded-2xl bg-[#2a3e00]/90 border border-[#c6ff00]/38 shadow-[0_0_18px_rgba(198,255,0,0.12)] pointer-events-none"
                        >
                          <Motion.span
                            initial={{ scaleY: 0.3, opacity: 0 }}
                            animate={{ scaleY: 1, opacity: 1 }}
                            exit={{ scaleY: 0.3, opacity: 0 }}
                            transition={SPRING_SNAPPY}
                            style={{ willChange: 'transform, opacity' }}
                            className="absolute left-0 top-1/2 -translate-y-1/2 w-[3.5px] h-7 rounded-r-full bg-[#c6ff00] shadow-[0_0_8px_rgba(198,255,0,0.75)]"
                          />
                        </Motion.div>
                      )}
                    </AnimatePresence>

                    {/* Avatar & Status Ring (exact 60px column) */}
                    <div className="w-[60px] h-[58px] flex items-center justify-center shrink-0 z-20 relative">
                      <Motion.div
                        animate={isSelected ? { scale: 1.04 } : { scale: 1 }}
                        transition={SPRING_SNAPPY}
                        style={{ willChange: 'transform' }}
                        className={`relative w-10 h-10 shrink-0 rounded-full transition-shadow duration-200 ${
                          instance.hasError
                            ? 'ring-2 ring-red-400/80'
                            : isSelected
                            ? 'ring-2 ring-[#c6ff00] shadow-[0_0_12px_rgba(198,255,0,0.38)]'
                            : instance.unread > 0
                            ? 'ring-2 ring-[#c6ff00]/80'
                            : 'ring-1 ring-[#474e3b] opacity-90 group-hover/session:opacity-100 group-hover/session:ring-[#c6ff00]/60'
                        }`}
                      >
                        <SessionAvatar
                          src={instance.avatar}
                          alt={instance.name}
                          className="w-full h-full rounded-full object-cover bg-[#21271a] select-none pointer-events-none"
                        />

                        {/* Ready / Error indicator dot */}
                        <span
                          className={`absolute -bottom-0.5 -right-0.5 w-3 h-3 rounded-full border-2 border-[#181c13] ${
                            instance.hasError
                              ? 'bg-red-400'
                              : instance.isReady
                              ? 'bg-[#c6ff00]'
                              : 'bg-[#737a67]'
                          }`}
                        />

                        {/* Collapsed-only Unread Badge on Avatar */}
                        <AnimatePresence>
                          {!isSidebarExpanded && instance.unread > 0 && (
                            <Motion.span
                              key="collapsed-unread"
                              initial={{ scale: 0, opacity: 0 }}
                              animate={{ scale: 1, opacity: 1 }}
                              exit={{ scale: 0, opacity: 0 }}
                              transition={SPRING_SNAPPY}
                              style={{ willChange: 'transform, opacity' }}
                              className="absolute -top-1.5 -right-1.5 min-w-[18px] h-[18px] px-1 rounded-full bg-[#c6ff00] text-[#182900] text-[10px] font-black flex items-center justify-center border-2 border-[#181c13] tabular-nums shadow-sm"
                            >
                              {instance.unread > 99 ? '99+' : instance.unread}
                            </Motion.span>
                          )}
                        </AnimatePresence>
                      </Motion.div>
                    </div>

                    {/* Expanded Session Label & Quick Actions (196px fits inside 256px expanded row even with 4px scrollbar) */}
                    <RevealLabel
                      show={isSidebarExpanded}
                      delay={isSidebarExpanded ? Math.min(idx * 0.015, 0.06) : 0}
                      className="w-[196px] shrink-0 flex items-center justify-between z-20 pr-2 overflow-hidden whitespace-nowrap"
                    >
                      <AnimatePresence mode="wait" initial={false}>
                        {isEditing ? (
                          <Motion.div
                            key="editing"
                            initial={{ opacity: 0, x: 8, scale: 0.97 }}
                            animate={{ opacity: 1, x: 0, scale: 1 }}
                            exit={{ opacity: 0, x: 6, scale: 0.97 }}
                            transition={{ duration: 0.14, ease: EASE_EXPAND }}
                            className="flex items-center gap-1 w-full whitespace-nowrap"
                            onClick={(e) => e.stopPropagation()}
                            onPointerDown={(e) => e.stopPropagation()}
                          >
                            <input
                              ref={renameInputRef}
                              type="text"
                              value={editingName}
                              maxLength={32}
                              onChange={(e) => setEditingName(e.target.value)}
                              onKeyDown={(e) => {
                                e.stopPropagation();
                                if (e.key === 'Enter') onSaveEditing();
                                if (e.key === 'Escape') onCancelEditing();
                              }}
                              className="h-8 bg-[#12150e] border border-[#c6ff00]/65 text-[#e7ebdc] text-xs px-2.5 rounded-xl outline-none w-full min-w-0"
                            />
                            <button
                              type="button"
                              onClick={onSaveEditing}
                              aria-label="Guardar nombre"
                              className="w-7 h-7 rounded-xl bg-[#c6ff00] text-[#182900] hover:bg-[#d8ff59] flex items-center justify-center shrink-0 cursor-pointer transition-colors"
                            >
                              <Check size={14} strokeWidth={2.6} />
                            </button>
                            <button
                              type="button"
                              onClick={onCancelEditing}
                              aria-label="Cancelar edición"
                              className="w-7 h-7 rounded-xl bg-[#21271a] text-[#a2a895] hover:text-[#e7ebdc] flex items-center justify-center shrink-0 cursor-pointer transition-colors"
                            >
                              <X size={14} />
                            </button>
                          </Motion.div>
                        ) : (
                          <Motion.div
                            key="normal"
                            initial={{ opacity: 0, x: -6 }}
                            animate={{ opacity: 1, x: 0 }}
                            exit={{ opacity: 0, x: -6 }}
                            transition={{ duration: 0.12, ease: EASE_EXPAND }}
                            className="flex items-center justify-between w-full min-w-0"
                          >
                            <div className="flex flex-col min-w-0 pr-1.5 overflow-hidden">
                              <span
                                className={`text-[13px] font-semibold truncate transition-colors duration-150 ${
                                  isSelected
                                    ? 'text-[#c6ff00]'
                                    : 'text-[#e7ebdc] group-hover/session:text-[#c6ff00]'
                                }`}
                              >
                                {instance.name}
                              </span>
                              <span className="text-[11px] text-[#a2a895] truncate">
                                {instance.hasError
                                  ? 'Error de red'
                                  : instance.unread > 0
                                  ? `${instance.unread} sin leer`
                                  : 'WhatsApp Web'}
                              </span>
                            </div>

                            <div className="flex items-center gap-0.5 shrink-0">
                              <AnimatePresence initial={false}>
                                {instance.unread > 0 && (
                                  <Motion.span
                                    key="expanded-unread"
                                    initial={{ scale: 0.5, opacity: 0 }}
                                    animate={{ scale: 1, opacity: 1 }}
                                    exit={{ scale: 0.5, opacity: 0 }}
                                    transition={SPRING_SNAPPY}
                                    style={{ willChange: 'transform, opacity' }}
                                    className="min-w-[20px] h-5 px-1.5 rounded-full bg-[#c6ff00] text-[#182900] text-[11px] font-black flex items-center justify-center tabular-nums shadow-sm"
                                  >
                                    {instance.unread > 99 ? '99+' : instance.unread}
                                  </Motion.span>
                                )}
                              </AnimatePresence>

                              <button
                                type="button"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  onStartEditing(instance);
                                }}
                                onPointerDown={(e) => e.stopPropagation()}
                                className="p-1.5 rounded-xl hover:bg-[#2b3222] text-[#a2a895] hover:text-[#c6ff00] active:scale-90 transition-all cursor-pointer opacity-80 group-hover/session:opacity-100"
                                title="Renombrar sesión"
                              >
                                <Pencil size={12.5} />
                              </button>

                              <button
                                type="button"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  onRequestDelete(instance.id);
                                }}
                                onPointerDown={(e) => e.stopPropagation()}
                                className="p-1.5 rounded-xl hover:bg-red-500/20 text-[#a2a895] hover:text-red-400 active:scale-90 transition-all cursor-pointer opacity-80 group-hover/session:opacity-100"
                                title="Eliminar sesión"
                              >
                                <Trash2 size={13} />
                              </button>

                              <div
                                className="p-1 text-[#474e3b] group-hover/session:text-[#a2a895] transition-colors cursor-grab active:cursor-grabbing shrink-0"
                                title="Arrastrar para reordenar"
                              >
                                <GripVertical size={13} />
                              </div>
                            </div>
                          </Motion.div>
                        )}
                      </AnimatePresence>
                    </RevealLabel>
                  </Reorder.Item>
                );
              })}
            </AnimatePresence>
          </Reorder.Group>

          {/* Expressive "+ Nueva sesión" CTA */}
          <Motion.button
            type="button"
            disabled={isAddingSession}
            onClick={onAddInstance}
            whileHover={{ scale: 1.014 }}
            whileTap={{ scale: 0.96 }}
            transition={SPRING_SNAPPY}
            title={!isSidebarExpanded ? 'Nueva sesión de WhatsApp' : undefined}
            className={`group/new relative w-full h-[58px] rounded-2xl flex items-center cursor-pointer overflow-hidden outline-none hover:bg-[#21271a] mt-1.5 transition-colors duration-150 ${
              isAddingSession ? 'opacity-65 cursor-wait' : ''
            }`}
          >
            <span className="absolute inset-0 rounded-2xl border border-transparent group-hover/new:border-[#c6ff00]/28 transition-colors pointer-events-none" />
            <div className="w-[60px] h-[58px] flex items-center justify-center shrink-0 z-20">
              <div className="w-10 h-10 shrink-0 rounded-2xl border border-dashed border-[#474e3b] bg-[#12150e]/50 flex items-center justify-center text-[#a2a895] transition-all duration-200 group-hover/new:border-[#c6ff00] group-hover/new:text-[#c6ff00] group-hover/new:bg-[#2a3e00]/55 group-hover/new:rotate-90">
                {isAddingSession ? (
                  <Motion.span
                    animate={{ rotate: 360 }}
                    transition={{ duration: 0.9, repeat: Infinity, ease: 'linear' }}
                    className="flex items-center justify-center text-[#c6ff00]"
                  >
                    <RefreshCw size={18} strokeWidth={2.4} />
                  </Motion.span>
                ) : (
                  <Plus size={20} strokeWidth={2.4} />
                )}
              </div>
            </div>

            <RevealLabel
              show={isSidebarExpanded}
              delay={isSidebarExpanded ? 0.02 : 0}
              className="w-[196px] shrink-0 flex items-center z-10 pr-3 overflow-hidden whitespace-nowrap"
            >
              <div className="flex flex-col text-left">
                <span className="text-[13px] font-semibold text-[#e7ebdc] group-hover/new:text-[#c6ff00] transition-colors duration-150">
                  {isAddingSession ? 'Creando sesión...' : 'Nueva sesión'}
                </span>
                <span className="text-[10.5px] text-[#737a67]">
                  Cuenta aislada
                </span>
              </div>
            </RevealLabel>
          </Motion.button>
        </div>

        {/* Settings Dock Item */}
        <div className="mt-auto pl-[10px] pr-[9px] pt-3 border-t border-[#2c3323]/80 z-10">
          <Motion.button
            type="button"
            onClick={() => onTabChange('settings')}
            whileHover={{ scale: 1.014 }}
            whileTap={{ scale: 0.96 }}
            transition={SPRING_SNAPPY}
            title={!isSidebarExpanded ? 'Preferencias y centro de control' : undefined}
            className={`group/settings relative w-full h-[58px] rounded-2xl flex items-center cursor-pointer overflow-hidden outline-none transition-colors duration-150 ${
              activeTab === 'settings' ? '' : 'hover:bg-[#21271a]'
            }`}
          >
            {activeTab !== 'settings' && (
              <span className="absolute inset-0 rounded-2xl border border-transparent group-hover/settings:border-[#2c3323] transition-colors pointer-events-none" />
            )}

            <AnimatePresence initial={false}>
              {activeTab === 'settings' && (
                <Motion.div
                  key="active-settings-pill"
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  exit={{ opacity: 0 }}
                  transition={{ duration: 0.16, ease: EASE_EXPAND }}
                  style={{ willChange: 'opacity' }}
                  className="absolute inset-0 rounded-2xl bg-[#2a3e00]/90 border border-[#c6ff00]/38 shadow-[0_0_18px_rgba(198,255,0,0.12)] pointer-events-none"
                >
                  <Motion.span
                    initial={{ scaleY: 0.3, opacity: 0 }}
                    animate={{ scaleY: 1, opacity: 1 }}
                    exit={{ scaleY: 0.3, opacity: 0 }}
                    transition={SPRING_SNAPPY}
                    style={{ willChange: 'transform, opacity' }}
                    className="absolute left-0 top-1/2 -translate-y-1/2 w-[3.5px] h-7 rounded-r-full bg-[#c6ff00] shadow-[0_0_8px_rgba(198,255,0,0.75)]"
                  />
                </Motion.div>
              )}
            </AnimatePresence>

            <div className="w-[60px] h-[58px] flex items-center justify-center shrink-0 z-20">
              <div
                className={`w-10 h-10 flex items-center justify-center rounded-2xl transition-all duration-200 ${
                  activeTab === 'settings'
                    ? 'bg-[#c6ff00] text-[#182900] shadow-md shadow-[#c6ff00]/25 group-hover/settings:rotate-45'
                    : 'bg-[#21271a] text-[#a2a895] group-hover/settings:text-[#c6ff00] group-hover/settings:bg-[#2b3222] group-hover/settings:rotate-45'
                }`}
              >
                <Settings size={20} strokeWidth={2.2} />
              </div>
            </div>

            <RevealLabel
              show={isSidebarExpanded}
              delay={isSidebarExpanded ? 0.025 : 0}
              className="w-[196px] shrink-0 flex items-center z-20 pr-3 overflow-hidden whitespace-nowrap"
            >
              <div className="flex flex-col text-left">
                <span
                  className={`text-[13px] font-semibold transition-colors duration-150 ${
                    activeTab === 'settings'
                      ? 'text-[#c6ff00]'
                      : 'text-[#e7ebdc] group-hover/settings:text-[#c6ff00]'
                  }`}
                >
                  Preferencias
                </span>
                <span className="text-[10.5px] text-[#737a67]">
                  Sistema y sesiones
                </span>
              </div>
            </RevealLabel>
          </Motion.button>
        </div>
      </div>
    </aside>
  );
}
