import React, { useEffect } from 'react';
import { motion as Motion, AnimatePresence } from 'framer-motion';
import { AlertTriangle, Trash2, X } from 'lucide-react';
import { EASE_EXIT, SPRING_SNAPPY } from './motionTokens';
import { SessionAvatar } from './UiPrimitives';

/**
 * Delete confirmation modal adapted to Supp's WebView2 geometry:
 * - When a WhatsApp session is active, main WebView is 280px wide -> renders compact 252px modal.
 * - When in Settings (100vw), renders centered 380px M3 dialog.
 * Uses only GPU transform + opacity (zero backdrop-filter blur).
 */
export function DeleteSessionModal({ session, isCompactViewport, onConfirm, onCancel }) {
  useEffect(() => {
    if (!session) return;
    const handleKeyDown = (e) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onCancel();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [session, onCancel]);

  return (
    <AnimatePresence>
      {session && (
        <Motion.div
          key="delete-session-modal-backdrop"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0, transition: { duration: 0.14, ease: EASE_EXIT } }}
          transition={{ duration: 0.16 }}
          onClick={onCancel}
          style={{ willChange: 'opacity' }}
          className="fixed inset-0 z-[10050] bg-[#0d1009]/82 flex items-center justify-center p-3 select-none"
        >
          <Motion.div
            key="delete-session-modal-card"
            role="dialog"
            aria-modal="true"
            aria-labelledby="delete-modal-title"
            initial={{ opacity: 0, scale: 0.9, y: 12 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{
              opacity: 0,
              scale: 0.92,
              y: 8,
              transition: { duration: 0.13, ease: EASE_EXIT },
            }}
            transition={SPRING_SNAPPY}
            onClick={(e) => e.stopPropagation()}
            style={{ willChange: 'transform, opacity' }}
            className={`bg-[#181c13] border border-red-500/35 m3-elevation-3 text-[#e7ebdc] relative ${
              isCompactViewport
                ? 'w-[252px] rounded-3xl p-4'
                : 'w-full max-w-[380px] rounded-[28px] p-6'
            }`}
          >
            <button
              type="button"
              onClick={onCancel}
              aria-label="Cerrar diálogo"
              className="absolute top-3 right-3 p-1.5 rounded-xl bg-[#21271a] text-[#a2a895] hover:text-[#e7ebdc] hover:bg-[#2b3222] transition-colors cursor-pointer"
            >
              <X size={14} />
            </button>

            <div className={`flex flex-col ${isCompactViewport ? 'items-center text-center' : 'items-start text-left'}`}>
              <div
                className={`rounded-2xl bg-red-500/15 border border-red-500/35 text-red-400 flex items-center justify-center ${
                  isCompactViewport ? 'w-11 h-11 mb-3' : 'w-12 h-12 mb-4'
                }`}
              >
                <AlertTriangle size={isCompactViewport ? 20 : 22} strokeWidth={2.3} />
              </div>

              <h3
                id="delete-modal-title"
                className={`font-bold text-[#e7ebdc] tracking-tight ${
                  isCompactViewport ? 'text-base mb-2' : 'text-xl mb-2'
                }`}
              >
                ¿Eliminar sesión?
              </h3>

              {/* Target session badge */}
              <div
                className={`flex items-center gap-2 rounded-2xl bg-[#21271a] border border-[#2c3323] px-2.5 py-1.5 max-w-full ${
                  isCompactViewport ? 'mb-2.5' : 'mb-3'
                }`}
              >
                <SessionAvatar
                  src={session.avatar}
                  alt={session.name}
                  className="w-5 h-5 rounded-full object-cover shrink-0 bg-[#181c13]"
                />
                <span className="text-xs font-semibold text-[#c6ff00] truncate">
                  {session.name}
                </span>
              </div>

              <p
                className={`text-[#a2a895] leading-relaxed ${
                  isCompactViewport ? 'text-[11px] mb-4' : 'text-xs mb-6'
                }`}
              >
                Se cerrará WhatsApp Web y se eliminarán los datos locales y la caché de esta cuenta.
              </p>

              <div className="flex items-center gap-2 w-full">
                <button
                  type="button"
                  onClick={onCancel}
                  className="flex-1 h-9 rounded-2xl bg-[#21271a] hover:bg-[#2b3222] border border-[#474e3b]/60 text-xs font-semibold text-[#e7ebdc] active:scale-[0.97] transition-all cursor-pointer"
                >
                  Cancelar
                </button>
                <button
                  type="button"
                  onClick={() => onConfirm(session.id)}
                  className="flex-1 h-9 rounded-2xl bg-red-500 hover:bg-red-400 text-xs font-bold text-white shadow-md shadow-red-950/50 active:scale-[0.97] transition-all cursor-pointer flex items-center justify-center gap-1.5"
                >
                  <Trash2 size={13} strokeWidth={2.4} />
                  <span>Eliminar</span>
                </button>
              </div>
            </div>
          </Motion.div>
        </Motion.div>
      )}
    </AnimatePresence>
  );
}
