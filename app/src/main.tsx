import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { init } from "./lib/store";
import "./styles.css";

// Block the browser context menu and reload shortcuts; this is an app, not a page.
window.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement;
  if (!t.closest("input, textarea, .code, .logbox, .detail")) e.preventDefault();
});
window.addEventListener("keydown", (e) => {
  if (e.key === "F5" || (e.ctrlKey && (e.key === "r" || e.key === "R"))) e.preventDefault();
});

init().catch((e) => console.error("LOTUS init failed", e));
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
