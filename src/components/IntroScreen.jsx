import React from 'react';
import { motion as Motion } from 'framer-motion';
import { MessageSquare, CheckCircle, RefreshCw } from 'lucide-react';
import { EASE_EMPHASIZED, EASE_EXPAND, SPRING_SNAPPY } from './motionTokens';
import { SessionAvatar } from './UiPrimitives';

export function IntroScreen({
  instances,
  startedSessionIds,
  readySessionIds,
  subProgress,
  allReady,
  exiting,
}) {
  const total = instances.length;
  const readyCount = instances.filter((i) => readySessionIds.has(i.id)).length;
  const activeLoadingCount = instances.filter(
    (i) => startedSessionIds.has(i.id) && !readySessionIds.has(i.id)
  ).length;

  const pendingFraction = activeLoadingCount > 0 ? Math.min(0.30, subProgress) : 0;

  const progress = allReady
    ? 100
    : total === 0
    ? Math.round(subProgress * 100)
    : Math.min(99, Math.round(((readyCount + pendingFraction) / total) * 100));

  const scaleProgress = Math.max(0, Math.min(1, progress / 100));

  return (
    <div
      key="supp-intro-screen"
      className="fixed inset-0 z-[999999] flex flex-col items-center justify-center bg-[#0d1009] text-[#e7ebdc] select-none overflow-hidden"
    >
      {/* Static ambient radial highlights (zero blur filters) */}
      <Motion.div
        animate={{ opacity: exiting ? 0 : 1 }}
        transition={{ duration: 0.18, ease: EASE_EMPHASIZED }}
        style={{ willChange: 'opacity' }}
        className="absolute inset-0 bg-[radial-gradient(ellipse_70%_58%_at_50%_42%,rgba(198,255,0,0.095)_0%,rgba(42,62,0,0.06)_42%,transparent_75%)] pointer-events-none"
      />

      <Motion.div
        initial={{ opacity: 0, y: 18, scale: 0.96 }}
        animate={
          exiting
            ? { opacity: 0, y: -12, scale: 0.96 }
            : { opacity: 1, y: 0, scale: 1 }
        }
        transition={{ duration: exiting ? 0.18 : 0.34, ease: EASE_EMPHASIZED }}
        style={{ willChange: 'transform, opacity' }}
        className="relative z-10 flex flex-col items-center max-w-md w-full px-6 text-center"
      >
        {/* Layered Orbital Emblem */}
        <div className="relative mb-7 w-28 h-28 flex items-center justify-center">
          {/* Outer orbital ring with glowing satellite dot */}
          <Motion.div
            animate={{ rotate: 360 }}
            transition={{ duration: 9, repeat: Infinity, ease: 'linear' }}
            style={{ willChange: 'transform' }}
            className="absolute inset-0 rounded-full border border-[#c6ff00]/18"
          >
            <span className="absolute -top-1 left-1/2 -translate-x-1/2 w-2 h-2 rounded-full bg-[#c6ff00] shadow-[0_0_10px_rgba(198,255,0,0.8)]" />
          </Motion.div>

          {/* Counter-rotating dashed squircle orbit */}
          <Motion.div
            animate={{ rotate: -360 }}
            transition={{ duration: 14, repeat: Infinity, ease: 'linear' }}
            style={{ willChange: 'transform' }}
            className="absolute inset-2.5 rounded-[28px] border border-dashed border-[#c6ff00]/35"
          />

          {/* Core lime squircle */}
          <Motion.div
            animate={allReady ? { scale: [1, 1.08, 1] } : { scale: [1, 1.03, 1] }}
            transition={{ duration: 1.8, repeat: Infinity, ease: 'easeInOut' }}
            style={{ willChange: 'transform' }}
            className="relative w-[68px] h-[68px] rounded-[22px] bg-[#c6ff00] text-[#182900] flex items-center justify-center shadow-[0_0_32px_rgba(198,255,0,0.32)]"
          >
            <MessageSquare size={32} strokeWidth={2.5} />
          </Motion.div>
        </div>

        {/* Title & Engine Pill */}
        <div className="flex items-center gap-2.5 mb-2">
          <h1 className="text-3xl font-extrabold tracking-tight text-[#e7ebdc]">Supp</h1>
          <span className="text-[10px] font-bold tracking-widest uppercase px-2.5 py-0.5 rounded-full bg-[#2a3e00] text-[#c6ff00] border border-[#c6ff00]/30">
            Rust Engine
          </span>
        </div>

        {/* Dynamic Status Subtitle */}
        <p className="text-xs font-medium text-[#a2a895] mb-5 h-4 flex items-center justify-center">
          {allReady
            ? '¡Sesiones listas para usar!'
            : total > 0
            ? `Preparando sesiones de WhatsApp (${readyCount} de ${total})...`
            : 'Iniciando entorno aislado...'}
        </p>

        {/* GPU-accelerated Progress Bar (scaleX transform only) */}
        <div className="w-72 mb-6">
          <div className="w-full h-2 rounded-full bg-[#181c13] border border-[#2c3323] p-[1px] overflow-hidden">
            <Motion.div
              initial={{ scaleX: 0 }}
              animate={{ scaleX: scaleProgress }}
              transition={{ duration: 0.2, ease: EASE_EXPAND }}
              style={{ transformOrigin: 'left center', willChange: 'transform' }}
              className="w-full h-full rounded-full bg-[#c6ff00] shadow-[0_0_12px_rgba(198,255,0,0.55)]"
            />
          </div>
          <div className="mt-2 flex items-center justify-between text-[11px] font-semibold tracking-wide text-[#737a67]">
            <span>
              {total > 0 ? `${readyCount}/${total} completadas` : 'Inicializando'}
            </span>
            <span className="text-[#c6ff00] tabular-nums font-bold">{progress}%</span>
          </div>
        </div>

        {/* Session Status Chips */}
        {total > 0 && (
          <div className="flex flex-wrap items-center justify-center gap-2 max-w-sm">
            {instances.map((inst) => {
              const isReady = readySessionIds.has(inst.id);
              const isCurrentLoading = startedSessionIds.has(inst.id) && !isReady;
              return (
                <Motion.div
                  key={inst.id}
                  initial={{ opacity: 0, y: 8, scale: 0.95 }}
                  animate={{
                    opacity: isReady || isCurrentLoading ? 1 : 0.52,
                    y: 0,
                    scale: isCurrentLoading ? 1.02 : 1,
                  }}
                  transition={SPRING_SNAPPY}
                  style={{ willChange: 'transform, opacity' }}
                  className={`flex items-center gap-2 px-3 py-1.5 rounded-2xl border text-xs font-semibold transition-colors duration-200 ${
                    isReady
                      ? 'bg-[#2a3e00]/75 border-[#c6ff00]/40 text-[#e7ebdc]'
                      : isCurrentLoading
                      ? 'bg-[#21271a] border-[#c6ff00]/55 text-[#e7ebdc] shadow-[0_0_14px_rgba(198,255,0,0.14)]'
                      : 'bg-[#181c13] border-[#2c3323] text-[#737a67]'
                  }`}
                >
                  <SessionAvatar
                    src={inst.avatar}
                    alt={inst.name}
                    className="w-5 h-5 rounded-full object-cover shrink-0 bg-[#21271a]"
                  />
                  <span className="truncate max-w-[110px]">{inst.name}</span>
                  <span className="w-3.5 h-3.5 flex items-center justify-center shrink-0">
                    {isReady ? (
                      <Motion.span
                        initial={{ scale: 0 }}
                        animate={{ scale: 1 }}
                        transition={SPRING_SNAPPY}
                        className="text-[#c6ff00] flex items-center justify-center"
                      >
                        <CheckCircle size={13} strokeWidth={2.6} />
                      </Motion.span>
                    ) : isCurrentLoading ? (
                      <Motion.span
                        animate={{ rotate: 360 }}
                        transition={{ duration: 1, repeat: Infinity, ease: 'linear' }}
                        style={{ willChange: 'transform' }}
                        className="text-[#c6ff00] flex items-center justify-center"
                      >
                        <RefreshCw size={12} strokeWidth={2.4} />
                      </Motion.span>
                    ) : (
                      <span className="w-2 h-2 rounded-full bg-[#3a422e]" title="En cola" />
                    )}
                  </span>
                </Motion.div>
              );
            })}
          </div>
        )}
      </Motion.div>
    </div>
  );
}
