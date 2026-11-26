import React from 'react';
import { motion as Motion, AnimatePresence } from 'framer-motion';
import { RefreshCw, CheckCircle, AlertTriangle } from 'lucide-react';
import { SPRING_SNAPPY, EASE_EXIT } from './motionTokens';

export function ToastPill({ notification }) {
  return (
    <AnimatePresence>
      {notification && (
        <Motion.div
          key="supp-toast-pill"
          initial={{ y: 22, opacity: 0, scale: 0.9 }}
          animate={{ y: 0, opacity: 1, scale: 1 }}
          exit={{ y: 14, opacity: 0, scale: 0.94, transition: { duration: 0.14, ease: EASE_EXIT } }}
          transition={SPRING_SNAPPY}
          style={{ willChange: 'transform, opacity' }}
          className="fixed bottom-6 right-6 z-[99999] pointer-events-none"
        >
          <div
            className={`bg-[#181c13] border rounded-full px-4 py-2.5 flex items-center gap-2.5 m3-elevation-3 ${
              notification.type === 'loading'
                ? 'border-[#c6ff00]/40'
                : notification.type === 'success'
                ? 'border-[#c6ff00]/55'
                : 'border-red-500/60'
            }`}
          >
            {notification.type === 'loading' && (
              <Motion.span
                animate={{ rotate: 360 }}
                transition={{ duration: 1, repeat: Infinity, ease: 'linear' }}
                style={{ willChange: 'transform' }}
                className="text-[#c6ff00] flex items-center justify-center shrink-0"
              >
                <RefreshCw size={14} strokeWidth={2.5} />
              </Motion.span>
            )}

            {notification.type === 'success' && (
              <span className="text-[#c6ff00] flex items-center justify-center shrink-0">
                <CheckCircle size={15} strokeWidth={2.5} />
              </span>
            )}

            {notification.type === 'error' && (
              <span className="text-red-400 flex items-center justify-center shrink-0">
                <AlertTriangle size={15} strokeWidth={2.5} />
              </span>
            )}

            <p className="text-xs font-medium text-[#e7ebdc] whitespace-nowrap tracking-[0.01em]">
              {notification.message}
            </p>
          </div>
        </Motion.div>
      )}
    </AnimatePresence>
  );
}
