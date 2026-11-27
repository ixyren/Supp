import React from 'react';
import { motion as Motion, AnimatePresence } from 'framer-motion';
import { EASE_EXPAND, EASE_EXIT, SPRING_SNAPPY, DEFAULT_AVATAR } from './motionTokens';

/**
 * Offline-safe session avatar that gracefully falls back to embedded SVG if an
 * external URL fails to load or is unreachable.
 */
export function SessionAvatar({ src, alt, className = '' }) {
  return (
    <img
      src={src || DEFAULT_AVATAR}
      alt={alt}
      referrerPolicy="no-referrer"
      onError={(e) => {
        if (e.currentTarget.src !== DEFAULT_AVATAR) {
          e.currentTarget.src = DEFAULT_AVATAR;
        }
      }}
      className={className}
    />
  );
}

/**
 * GPU-composited reveal label for sidebar items.
 * Zero collapse delay prevents text clipping or ghosting on rapid mouse-leave.
 */
export function RevealLabel({ show, delay = 0, className = '', children }) {
  return (
    <Motion.div
      initial={false}
      animate={show ? { opacity: 1, x: 0 } : { opacity: 0, x: -10 }}
      transition={
        show
          ? { duration: 0.22, ease: EASE_EXPAND, delay }
          : { duration: 0.11, ease: EASE_EXIT, delay: 0 }
      }
      style={{
        pointerEvents: show ? 'auto' : 'none',
        willChange: 'transform, opacity',
      }}
      className={className}
    >
      {children}
    </Motion.div>
  );
}

/**
 * Material You 3 Expressive Switch with spring thumb morph and center state dot.
 */
export function MaterialSwitch({ active, onClick, ariaLabel }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={active}
      aria-label={ariaLabel}
      onClick={onClick}
      className={`relative w-[54px] h-[32px] rounded-full p-1 transition-colors duration-200 outline-none cursor-pointer flex items-center shrink-0 border ${
        active
          ? 'bg-[#c6ff00] border-[#d8ff59]/60 shadow-[0_0_16px_rgba(198,255,0,0.22)]'
          : 'bg-[#21271a] border-[#474e3b] hover:border-[#626b52]'
      }`}
    >
      <Motion.div
        layout
        whileTap={{ scaleX: 1.22, scaleY: 0.92 }}
        transition={SPRING_SNAPPY}
        style={{ willChange: 'transform' }}
        className={`w-[22px] h-[22px] rounded-full shadow-md flex items-center justify-center ${
          active ? 'bg-[#182900] ml-auto' : 'bg-[#a2a895] ml-0'
        }`}
      >
        <AnimatePresence initial={false}>
          {active ? (
            <Motion.span
              key="dot-on"
              initial={{ scale: 0, opacity: 0 }}
              animate={{ scale: 1, opacity: 1 }}
              exit={{ scale: 0, opacity: 0 }}
              transition={SPRING_SNAPPY}
              className="w-2 h-2 rounded-full bg-[#c6ff00]"
            />
          ) : (
            <Motion.span
              key="dot-off"
              initial={{ scale: 0, opacity: 0 }}
              animate={{ scale: 1, opacity: 0.55 }}
              exit={{ scale: 0, opacity: 0 }}
              transition={SPRING_SNAPPY}
              className="w-1.5 h-1.5 rounded-full bg-[#181c13]"
            />
          )}
        </AnimatePresence>
      </Motion.div>
    </button>
  );
}
