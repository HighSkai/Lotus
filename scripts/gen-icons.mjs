// Generates tray icon SVGs (one per state) into app/src-tauri/icons/tray-src.
// PNGs are rendered from them with `tauri icon --png 32` (see scripts/build.ps1).
import { mkdirSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(root, "app", "src-tauri", "icons", "tray-src");
mkdirSync(out, { recursive: true });

const petalC = "M32 6 C40.5 16 42 29 32 44 C22 29 23.5 16 32 6 Z";
const petalS = "M32 13 C38.5 21 39.5 31 32 44 C24.5 31 25.5 21 32 13 Z";

function lotus({ top, bottom, side, ring }) {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
  <defs><linearGradient id="g" x1="0.5" y1="0" x2="0.5" y2="1"><stop offset="0" stop-color="${top}"/><stop offset="1" stop-color="${bottom}"/></linearGradient></defs>
  <g transform="translate(32 34) scale(1.18) translate(-32 -30)">
    <path d="${petalS}" fill="${side}" transform="rotate(-40 32 44)"/>
    <path d="${petalS}" fill="${side}" transform="rotate(40 32 44)"/>
    <path d="${petalC}" fill="url(#g)"/>
  </g>
  ${ring ? `<circle cx="51" cy="51" r="9" fill="${ring}" stroke="#FFFFFF" stroke-width="3"/>` : ""}
</svg>`;
}

const states = {
  off: lotus({ top: "#A9AECB", bottom: "#7C819F", side: "#9BA0BC" }),
  lotus: lotus({ top: "#F59BC2", bottom: "#7D93FF", side: "#B7A7FF" }),
  shadow: lotus({ top: "#B08CFF", bottom: "#4A55E0", side: "#6E5BD8" }),
  busy: lotus({ top: "#B9BEDD", bottom: "#8A90B2", side: "#A9AECB", ring: "#7C8CFF" }),
  error: lotus({ top: "#A9AECB", bottom: "#7C819F", side: "#9BA0BC", ring: "#F0567A" }),
};
for (const [name, svg] of Object.entries(states)) writeFileSync(join(out, `${name}.svg`), svg);
console.log(`wrote ${Object.keys(states).length} tray SVGs to ${out}`);
