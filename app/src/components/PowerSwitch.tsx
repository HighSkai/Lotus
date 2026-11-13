import { motion } from "motion/react";
import { LotusMark } from "./LotusMark";
import { easeOut } from "@/lib/motion";
import type { Mode } from "@/lib/types";

export type Visual = "off" | "connecting" | "on" | "reconnecting" | "disconnecting" | "error";

const LABEL: Record<Visual, string> = {
  off: "Turn on",
  connecting: "Connecting",
  on: "Turn off",
  reconnecting: "Reconnecting, turn off",
  disconnecting: "Turning off",
  error: "Try again",
};

// Geometry mirrors .power in styles.css: width 184, knob 82, padding 9.
const TRAVEL = 184 - 82 - 9 * 2;

const KNOB_X: Record<Visual, number> = { off: 0, error: 0, connecting: TRAVEL / 2, disconnecting: TRAVEL / 2, on: TRAVEL, reconnecting: TRAVEL };
const FILL: Record<Visual, { opacity: number; scaleX: number }> = {
  off: { opacity: 0, scaleX: 0.6 },
  error: { opacity: 0, scaleX: 0.6 },
  connecting: { opacity: 0.45, scaleX: 0.85 },
  disconnecting: { opacity: 0.45, scaleX: 0.85 },
  reconnecting: { opacity: 0.7, scaleX: 1 },
  on: { opacity: 1, scaleX: 1 },
};

const knobSpring = { type: "spring", stiffness: 460, damping: 30, mass: 0.9 } as const;

export function PowerSwitch({ visual, mode, onToggle, disabled }: { visual: Visual; mode: Mode; onToggle: () => void; disabled?: boolean }) {
  const checked = visual === "on" || visual === "reconnecting";
  const busy = visual === "connecting" || visual === "disconnecting" || visual === "reconnecting";
  const off = disabled || visual === "disconnecting";
  return (
    <motion.button
      type="button"
      className="power"
      data-state={visual}
      data-busy={busy}
      role="switch"
      aria-checked={checked}
      aria-busy={busy}
      aria-label={`${mode === "lotus" ? "LOTUS" : "SHADOW"}: ${LABEL[visual]}`}
      disabled={off}
      onClick={onToggle}
      whileHover={off ? undefined : { scale: 1.015 }}
      whileTap={off ? undefined : { scale: 0.965 }}
      transition={{ type: "spring", stiffness: 600, damping: 32 }}
      style={{ cursor: off ? "default" : "pointer" }}
    >
      <span className="power-track">
        <motion.span className="power-fill" style={{ originX: 0 }} initial={false} animate={FILL[visual]} transition={{ duration: 0.34, ease: easeOut }} />
        <span className="power-shimmer" />
      </span>
      {/* Outer layer shakes on failure; the knob itself springs between positions. */}
      <motion.span
        className="absolute inset-0"
        initial={false}
        animate={visual === "error" ? { x: [0, -6, 5, -3, 0] } : { x: 0 }}
        transition={visual === "error" ? { duration: 0.42, ease: "easeOut", delay: 0.12 } : { duration: 0 }}
      >
        <motion.span className="power-knob" initial={false} animate={{ x: KNOB_X[visual] }} transition={knobSpring}>
          <span className="ring" />
          <LotusMark mode={mode} />
        </motion.span>
      </motion.span>
    </motion.button>
  );
}
