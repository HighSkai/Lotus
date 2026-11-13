import { useEffect } from "react";
import { AnimatePresence, animate, motion, useMotionValue, useMotionTemplate, useTransform } from "motion/react";
import { LotusMark } from "./LotusMark";
import { ShadowMark } from "./ShadowMark";
import { easeOut, glide, instant, spring, useMotionLevel } from "@/lib/motion";
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
const W = 184;
const K = 82;
const PAD = 9;
const TRAVEL = W - K - PAD * 2;

const KNOB_X: Record<Visual, number> = { off: 0, error: 0, connecting: TRAVEL / 2, disconnecting: TRAVEL / 2, on: TRAVEL, reconnecting: TRAVEL };

// Arrives in ~250 ms with a slight settle.
const knobSpring = { type: "spring", stiffness: 640, damping: 38, mass: 0.8 } as const;

export function PowerSwitch({ visual, mode, onToggle, disabled }: { visual: Visual; mode: Mode; onToggle: () => void; disabled?: boolean }) {
  const level = useMotionLevel();
  const checked = visual === "on" || visual === "reconnecting";
  const busy = visual === "connecting" || visual === "disconnecting" || visual === "reconnecting";
  const off = disabled || visual === "disconnecting";

  // One value drives both the knob and the colour, so they can never drift apart.
  const x = useMotionValue(KNOB_X[visual]);
  useEffect(() => {
    const c = animate(x, KNOB_X[visual], level === "full" ? knobSpring : level === "reduce" ? glide : instant);
    return () => c.stop();
  }, [visual, level, x]);

  // The colour is solid up to the knob, then fades out over a soft edge that
  // shrinks to nothing as the switch reaches "on" (so "on" is fully coloured).
  const p = useTransform(x, [0, TRAVEL], [0, 1], { clamp: true });
  const solid = useTransform(() => PAD + K / 2 + x.get() + p.get() * (W - PAD - K / 2 - TRAVEL));
  const clear = useTransform(() => solid.get() + 72 * (1 - p.get()));
  const mask = useMotionTemplate`linear-gradient(90deg, #000 ${solid}px, transparent ${clear}px)`;
  const fillOpacity = useTransform(x, [0, TRAVEL * 0.18], [0, 1], { clamp: true });
  // A reconnecting switch sits at "on" but looks less certain.
  const tone = visual === "reconnecting" ? 0.72 : 1;

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
      whileTap={off || level !== "full" ? undefined : { scale: 0.965 }}
      transition={{ type: "spring", stiffness: 800, damping: 40 }}
      style={{ cursor: off ? "default" : "pointer" }}
    >
      <span className="power-track">
        <motion.span className="power-tone" initial={false} animate={{ opacity: tone }} transition={level === "none" ? instant : { duration: 0.2, ease: easeOut }}>
          <motion.span className="power-fill" style={{ opacity: fillOpacity, maskImage: mask, WebkitMaskImage: mask }} />
        </motion.span>
        <span className="power-shimmer" />
      </span>
      {/* Outer layer shakes on failure (full motion only); the knob glides between positions. */}
      <motion.span
        className="absolute inset-0"
        initial={false}
        animate={visual === "error" && level === "full" ? { x: [0, -6, 5, -3, 0] } : { x: 0 }}
        transition={visual === "error" && level === "full" ? { duration: 0.42, ease: "easeOut", delay: 0.12 } : instant}
      >
        <motion.span className="power-knob" style={{ x }}>
          <span className="ring" />
          {/* LOTUS shows the flower; SHADOW the flower in a crescent moon. */}
          <AnimatePresence initial={false} mode="popLayout">
            <motion.span
              key={mode}
              className="grid place-items-center"
              initial={level === "full" ? { opacity: 0, scale: 0.6, rotate: -45 } : { opacity: 0 }}
              animate={{ opacity: 1, scale: 1, rotate: 0 }}
              exit={level === "full" ? { opacity: 0, scale: 0.6, rotate: 45 } : { opacity: 0 }}
              transition={level === "full" ? spring : level === "reduce" ? { duration: 0.18, ease: "easeOut" } : instant}
            >
              {mode === "lotus" ? <LotusMark mode="lotus" /> : <ShadowMark lit={checked} />}
            </motion.span>
          </AnimatePresence>
        </motion.span>
      </motion.span>
    </motion.button>
  );
}
