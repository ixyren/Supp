// Motion tokens strictly synchronized with Rust native bounds animations (src-tauri/src/lib.rs)
// Only GPU-composited properties (transform / opacity) — zero filter: blur(...) animations.
export const EASE_EXPAND = [0.16, 1, 0.3, 1];     // 220ms expand
export const EASE_COLLAPSE = [0.2, 0.9, 0.3, 1];  // 160ms collapse
export const EASE_SLIDE = [0.16, 1, 0.3, 1];      // 240ms slide
export const EASE_EMPHASIZED = [0.16, 1, 0.3, 1];
export const EASE_EXIT = [0.3, 0, 0.8, 0.15];

export const SPRING_SOFT = { type: 'spring', stiffness: 420, damping: 32, mass: 0.7 };
export const SPRING_SNAPPY = { type: 'spring', stiffness: 520, damping: 28, mass: 0.6 };
export const SPRING_LAYOUT = { type: 'spring', stiffness: 480, damping: 34, mass: 0.65 };

// 100% offline-safe Material You 3 Dark-Lime default session avatar SVG
export const DEFAULT_AVATAR =
  "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 96 96'%3E%3Cdefs%3E%3ClinearGradient id='g' x1='0' y1='0' x2='1' y2='1'%3E%3Cstop offset='0%25' stop-color='%232a3e00'/%3E%3Cstop offset='100%25' stop-color='%23181c13'/%3E%3C/linearGradient%3E%3C/defs%3E%3Crect width='96' height='96' rx='48' fill='url(%23g)'/%3E%3Ccircle cx='48' cy='37' r='15' fill='%23c6ff00' fill-opacity='0.88'/%3E%3Cpath d='M22 80c3-15 14-23 26-23s23 8 26 23' fill='%23c6ff00' fill-opacity='0.88'/%3E%3C/svg%3E";
