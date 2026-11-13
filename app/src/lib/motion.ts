import { useSyncExternalStore } from "react";
import { useStore } from "./store";
import type { AppSettings } from "./types";

// Shared motion presets. Springs are tuned close to critical damping so things
// arrive fast (~250 ms) without wobbling.
export const spring = { type: "spring", stiffness: 720, damping: 52, mass: 0.7 } as const;
export const softSpring = { type: "spring", stiffness: 380, damping: 34, mass: 0.9 } as const;
export const easeOut = [0.22, 1, 0.36, 1] as const;
export const fade = { duration: 0.14, ease: easeOut } as const;

/**
 * How much the interface moves.
 * - full: springs, slides and small flourishes.
 * - reduce: the same changes as short, smooth glides and fades; nothing
 *   bounces, rotates, scales or loops.
 * - none: every change is instant.
 */
export type MotionLevel = "full" | "reduce" | "none";

const query = typeof window !== "undefined" ? window.matchMedia("(prefers-reduced-motion: reduce)") : null;
const subscribe = (cb: () => void) => {
  query?.addEventListener("change", cb);
  return () => query?.removeEventListener("change", cb);
};

export function resolveMotion(pref: AppSettings["motion"] | undefined, osReduce: boolean): MotionLevel {
  if (pref === "full" || pref === "reduce" || pref === "none") return pref;
  return osReduce ? "reduce" : "full";
}

export function useMotionLevel(): MotionLevel {
  const pref = useStore((s) => s.settings?.motion);
  const osReduce = useSyncExternalStore(subscribe, () => query?.matches ?? false);
  return resolveMotion(pref, osReduce);
}

/** A calm glide for "Less": no overshoot, settles in 200 ms. */
export const glide = { type: "tween", duration: 0.2, ease: easeOut } as const;
export const instant = { duration: 0 } as const;

/** Enter/exit for small swaps (headings, stat lines, cards). */
export function swapMotion(level: MotionLevel, rise = 5) {
  if (level === "full") {
    return { initial: { opacity: 0, y: rise }, animate: { opacity: 1, y: 0 }, exit: { opacity: 0, y: -rise * 0.6 }, transition: { duration: 0.14, ease: easeOut } } as const;
  }
  return { initial: { opacity: 0 }, animate: { opacity: 1 }, exit: { opacity: 0 }, transition: level === "none" ? instant : ({ duration: 0.16, ease: "easeOut" } as const) } as const;
}
