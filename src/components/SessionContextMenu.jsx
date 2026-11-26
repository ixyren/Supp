import React from 'react';
import { motion as Motion, AnimatePresence } from 'framer-motion';
import { Pencil, Trash2, ExternalLink, RefreshCw } from 'lucide-react';
import { EASE_EXIT, SPRING_SNAPPY } from './motionTokens';

export function SessionContextMenu({
  contextMenu,
  menuRef,
  activeTab,
  isCompactViewport,
  canReloadSession,
  onOpenSession,
  onRenameSession,
  onReloadSession,
  onDeleteSession,
}) {
  const menuWidth = 186;
  const showOpenAction = Boolean(contextMenu && activeTab !== contextMenu.session.id);
  const estimatedHeight =
    96 + (showOpenAction ? 36 : 0) + (canReloadSession ? 36 : 0) + 40;
  const maxX = isCompactViewport
    ? Math.max(8, 280 - menuWidth - 10)
    : Math.max(8, window.innerWidth - menuWidth - 12);
  const minX = isCompactViewport ? 72 : 12;

  const clampedX = contextMenu
    ? Math.min(Math.max(contextMenu.x, minX), maxX)
    : 0;
  const clampedY = contextMenu
    ? Math.min(
        Math.max(contextMenu.y, 10),
        Math.max(10, window.innerHeight - estimatedHeight)
      )
    : 0;

  return (
    <AnimatePresence>
      {contextMenu && (
        <Motion.div
          ref={menuRef}
          key="supp-session-context-menu"
          initial={{ opacity: 0, scale: 0.88, y: -4 }}
          animate={{ opacity: 1, scale: 1, y: 0 }}
          exit={{ opacity: 0, scale: 0.92, y: -4, transition: { duration: 0.11, ease: EASE_EXIT } }}
          transition={SPRING_SNAPPY}
          style={{
            position: 'fixed',
            left: `${clampedX}px`,
            top: `${clampedY}px`,
            width: `${menuWidth}px`,
            transformOrigin: 'top left',
            willChange: 'transform, opacity',
          }}
          className="bg-[#181c13] border border-[#c6ff00]/35 rounded-2xl m3-elevation-3 p-1.5 z-[10000] select-none"
          onClick={(e) => e.stopPropagation()}
        >
          {/* Header */}
          <div className="px-2.5 py-1.5 mb-1 border-b border-[#2c3323] flex items-center gap-2">
            <span className="w-2 h-2 rounded-full bg-[#c6ff00] shrink-0" />
            <span className="text-xs font-bold text-[#e7ebdc] truncate">
              {contextMenu.session.name}
            </span>
          </div>

          {activeTab !== contextMenu.session.id && (
            <button
              type="button"
              onClick={() => onOpenSession(contextMenu.session)}
              className="w-full flex items-center gap-2.5 px-2.5 py-2 text-xs font-medium text-[#e7ebdc] hover:text-[#c6ff00] hover:bg-[#21271a] rounded-xl transition-colors cursor-pointer group"
            >
              <ExternalLink size={13} className="text-[#a2a895] group-hover:text-[#c6ff00] transition-colors shrink-0" />
              <span>Abrir sesión</span>
            </button>
          )}

          <button
            type="button"
            onClick={() => onRenameSession(contextMenu.session)}
            className="w-full flex items-center gap-2.5 px-2.5 py-2 text-xs font-medium text-[#e7ebdc] hover:text-[#c6ff00] hover:bg-[#21271a] rounded-xl transition-colors cursor-pointer group"
          >
            <Pencil size={13} className="text-[#a2a895] group-hover:text-[#c6ff00] transition-colors shrink-0" />
            <span>Renombrar</span>
          </button>

          {canReloadSession && (
            <button
              type="button"
              onClick={() => onReloadSession(contextMenu.session)}
              className="w-full flex items-center gap-2.5 px-2.5 py-2 text-xs font-medium text-[#e7ebdc] hover:text-[#c6ff00] hover:bg-[#21271a] rounded-xl transition-colors cursor-pointer group"
            >
              <RefreshCw size={13} className="text-[#a2a895] group-hover:text-[#c6ff00] transition-colors shrink-0" />
              <span>Recargar</span>
            </button>
          )}

          <div className="my-1 border-t border-[#2c3323]" />

          <button
            type="button"
            onClick={() => onDeleteSession(contextMenu.session)}
            className="w-full flex items-center gap-2.5 px-2.5 py-2 text-xs font-medium text-red-400 hover:text-red-300 hover:bg-red-950/45 rounded-xl transition-colors cursor-pointer group"
          >
            <Trash2 size={13} className="text-red-400 group-hover:text-red-300 transition-colors shrink-0" />
            <span>Eliminar sesión</span>
          </button>
        </Motion.div>
      )}
    </AnimatePresence>
  );
}
