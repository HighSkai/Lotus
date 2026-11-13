import type { MouseEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

// Anything a click should reach instead of moving the window.
const INTERACTIVE = "button, a, input, textarea, select, label, [role=button], [role=switch], [role=radio], [role=tab], [data-no-drag]";

/** Mouse-down handler for the top strip: drag the window from any empty spot. */
export function startWindowDrag(e: MouseEvent) {
  if (e.button !== 0 || e.defaultPrevented) return;
  if ((e.target as Element).closest(INTERACTIVE)) return;
  e.preventDefault();
  try {
    void getCurrentWindow().startDragging().catch(() => {});
  } catch {
    // Not running inside Tauri (plain browser preview).
  }
}

export function minimizeWindow() {
  try {
    void getCurrentWindow().minimize().catch(() => {});
  } catch {
    // Not running inside Tauri.
  }
}
