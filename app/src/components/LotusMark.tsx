import { useId } from "react";
import type { Mode } from "../lib/types";

const CENTER = "M32 6 C40.5 16 42 29 32 44 C22 29 23.5 16 32 6 Z";
const SIDE = "M32 13 C38.5 21 39.5 31 32 44 C24.5 31 25.5 21 32 13 Z";
const OUTER = "M32 20 C36.5 26 37 34 32 44 C27 34 27.5 26 32 20 Z";

const PALETTE: Record<Mode, { top: string; mid: string; bottom: string; side: string; outer: string; water: string }> = {
  lotus: { top: "#F49AC1", mid: "#B49CFF", bottom: "#7F95FF", side: "#C9B8FF", outer: "#F6B9D3", water: "#8D9BFF" },
  shadow: { top: "#C6A8FF", mid: "#7C62F0", bottom: "#3F4AD6", side: "#6E5BD8", outer: "#4B3FB8", water: "#6E7BFF" },
};

/** The LOTUS mark. `mode` switches between the dawn and midnight palettes. */
export function LotusMark({ mode, className, water = false }: { mode: Mode; className?: string; water?: boolean }) {
  const id = useId().replace(/:/g, "");
  const c = PALETTE[mode];
  return (
    <svg className={className} viewBox="0 0 64 64" aria-hidden="true">
      <defs>
        <linearGradient id={`p${id}`} x1="0.5" y1="0" x2="0.5" y2="1">
          <stop offset="0" stopColor={c.top} />
          <stop offset="0.55" stopColor={c.mid} />
          <stop offset="1" stopColor={c.bottom} />
        </linearGradient>
      </defs>
      <g transform="translate(32 36) scale(1.05) translate(-32 -32)">
        <path d={OUTER} fill={c.outer} opacity="0.5" transform="rotate(-72 32 44)" />
        <path d={OUTER} fill={c.outer} opacity="0.5" transform="rotate(72 32 44)" />
        <path d={SIDE} fill={c.side} opacity="0.85" transform="rotate(-38 32 44)" />
        <path d={SIDE} fill={c.side} opacity="0.85" transform="rotate(38 32 44)" />
        <path d={CENTER} fill={`url(#p${id})`} />
        <path d="M32 9 C35 15 36 22 34.5 30" stroke="#fff" strokeOpacity="0.5" strokeWidth="1.2" fill="none" strokeLinecap="round" />
        {water && <path d="M17 50 Q32 55.5 47 50" stroke={c.water} strokeOpacity="0.6" strokeWidth="2" fill="none" strokeLinecap="round" />}
      </g>
    </svg>
  );
}
