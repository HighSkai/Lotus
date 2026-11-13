import { useId } from "react";
import { motion } from "motion/react";
import { LotusMark } from "./LotusMark";
import type { Mode } from "@/lib/types";

const CENTER = "M32 6 C40.5 16 42 29 32 44 C22 29 23.5 16 32 6 Z";
const SIDE = "M32 13 C38.5 21 39.5 31 32 44 C24.5 31 25.5 21 32 13 Z";
const OUTER = "M32 20 C36.5 26 37 34 32 44 C27 34 27.5 26 32 20 Z";

const swing = { type: "spring", stiffness: 260, damping: 22, mass: 0.9 } as const;

/**
 * The SHADOW mark: the lotus resting in a crescent moon. `lit` swings the
 * crescent into place and lifts the flower, used when SHADOW is connected.
 */
export function ShadowMark({ className, lit = false }: { className?: string; lit?: boolean }) {
  const id = useId().replace(/:/g, "");
  return (
    <svg className={className} viewBox="0 0 64 64" aria-hidden="true">
      <defs>
        <linearGradient id={`sp${id}`} x1="0.5" y1="0" x2="0.5" y2="1">
          <stop offset="0" stopColor="#B9A8FF" />
          <stop offset="0.55" stopColor="#6E5CE6" />
          <stop offset="1" stopColor="#3A3FB6" />
        </linearGradient>
        <linearGradient id={`sm${id}`} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#A493F7" />
          <stop offset="1" stopColor="#4A4FC4" />
        </linearGradient>
        <mask id={`sk${id}`}>
          <rect width="64" height="64" fill="#fff" />
          <circle cx="32" cy="27" r="22" fill="#000" />
        </mask>
      </defs>
      <motion.g initial={false} animate={{ rotate: lit ? 0 : -28, opacity: lit ? 1 : 0.9 }} transition={swing}>
        <circle cx="32" cy="33" r="23" fill={`url(#sm${id})`} mask={`url(#sk${id})`} />
      </motion.g>
      <motion.g initial={false} animate={{ y: lit ? 0 : 1.5 }} transition={swing}>
        <g transform="translate(32 33) scale(0.82) translate(-32 -32)">
          <path d={OUTER} fill="#3D3896" opacity="0.55" transform="rotate(-72 32 44)" />
          <path d={OUTER} fill="#3D3896" opacity="0.55" transform="rotate(72 32 44)" />
          <path d={SIDE} fill="#5A4FCF" opacity="0.9" transform="rotate(-38 32 44)" />
          <path d={SIDE} fill="#5A4FCF" opacity="0.9" transform="rotate(38 32 44)" />
          <path d={CENTER} fill={`url(#sp${id})`} />
          <path d="M32 9 C35 15 36 22 34.5 30" stroke="#fff" strokeOpacity="0.55" strokeWidth="1.2" fill="none" strokeLinecap="round" />
        </g>
      </motion.g>
    </svg>
  );
}

/** The current mode's mark: the lotus for LOTUS, the lotus and crescent for SHADOW. */
export function ModeMark({ mode, className, water }: { mode: Mode; className?: string; water?: boolean }) {
  return mode === "shadow" ? <ShadowMark className={className} lit /> : <LotusMark mode="lotus" className={className} water={water} />;
}
