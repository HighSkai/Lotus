import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { init } from "./lib/store";
import "./styles.css";

// Block the browser context menu and reload shortcuts; this is an app, not a page.
window.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement;
  if (!t.closest("input, textarea, .selectable")) e.preventDefault();
});
window.addEventListener("keydown", (e) => {
  if (e.key === "F5" || (e.ctrlKey && (e.key === "r" || e.key === "R"))) e.preventDefault();
});

// Pointer-following highlight on [data-spot] surfaces (see styles.css).
let spotFrame = 0;
window.addEventListener(
  "pointermove",
  (e) => {
    if (spotFrame) return;
    spotFrame = requestAnimationFrame(() => {
      spotFrame = 0;
      const el = (e.target as Element | null)?.closest?.("[data-spot]") as HTMLElement | null;
      if (!el) return;
      const r = el.getBoundingClientRect();
      el.style.setProperty("--mx", `${e.clientX - r.left}px`);
      el.style.setProperty("--my", `${e.clientY - r.top}px`);
    });
  },
  { passive: true },
);

async function start() {
  // Plain-browser UI preview (`npm run dev` outside Tauri) runs against fake data.
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) await (await import("./dev/mock")).installMock();
  await init();
}

start().catch((e) => console.error("LOTUS init failed", e));
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
