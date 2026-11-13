import { LotusMark } from "./LotusMark";
import type { Mode } from "../lib/types";

export type Visual = "off" | "connecting" | "on" | "reconnecting" | "disconnecting" | "error";

const LABEL: Record<Visual, string> = {
  off: "Turn on",
  connecting: "Connecting",
  on: "Turn off",
  reconnecting: "Reconnecting, turn off",
  disconnecting: "Turning off",
  error: "Try again",
};

export function PowerSwitch({ visual, mode, onToggle, disabled }: { visual: Visual; mode: Mode; onToggle: () => void; disabled?: boolean }) {
  const checked = visual === "on" || visual === "reconnecting";
  return (
    <button
      type="button"
      className="power"
      data-state={visual}
      role="switch"
      aria-checked={checked}
      aria-busy={visual === "connecting" || visual === "disconnecting" || visual === "reconnecting"}
      aria-label={`${mode === "lotus" ? "LOTUS" : "SHADOW"}: ${LABEL[visual]}`}
      disabled={disabled || visual === "disconnecting"}
      onClick={onToggle}
    >
      <span className="power-track">
        <span className="power-fill" />
        <span className="power-shimmer" />
      </span>
      <span className="power-knob">
        <span className="ring" />
        <LotusMark mode={mode} />
      </span>
    </button>
  );
}
