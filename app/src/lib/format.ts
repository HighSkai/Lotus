import type { ShadowOptions, SplitTunnel } from "./types";

export function rate(bytesPerSec: number): string {
  if (bytesPerSec < 1024) return `${bytesPerSec} B/s`;
  if (bytesPerSec < 1024 * 1024) return `${(bytesPerSec / 1024).toFixed(bytesPerSec < 10 * 1024 ? 1 : 0)} KB/s`;
  return `${(bytesPerSec / 1024 / 1024).toFixed(1)} MB/s`;
}

export function bytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
}

/** A byte count split into a number and unit, for animated counters. */
export function byteParts(n: number): { value: number; unit: string; digits: number } {
  if (n < 1024) return { value: n, unit: "B", digits: 0 };
  if (n < 1024 ** 2) {
    const v = n / 1024;
    return { value: v, unit: "KB", digits: v < 10 ? 1 : 0 };
  }
  if (n < 1024 ** 3) {
    const v = n / 1024 ** 2;
    return { value: v, unit: "MB", digits: v < 100 ? 1 : 0 };
  }
  return { value: n / 1024 ** 3, unit: "GB", digits: 2 };
}

/** Whole hours, minutes and seconds of a duration. */
export function clockParts(ms: number): { h: number; m: number; s: number } {
  const t = Math.max(0, Math.floor(ms / 1000));
  return { h: Math.floor(t / 3600), m: Math.floor((t % 3600) / 60), s: t % 60 };
}

export function duration(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(sec)}` : `${pad(m)}:${pad(sec)}`;
}

const regionNames = (() => {
  try {
    return new Intl.DisplayNames(["en"], { type: "region" });
  } catch {
    return null;
  }
})();

export function country(code: string | null | undefined): string | null {
  if (!code) return null;
  try {
    return regionNames?.of(code.toUpperCase()) ?? code;
  } catch {
    return code;
  }
}

export function hostLabel(server: string, port: number): string {
  return `${server.includes(":") ? `[${server}]` : server}:${port}`;
}


const count = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** "2 apps and 1 site". */
export function splitItems(s: SplitTunnel): string {
  const a = s.apps.length;
  const x = s.sites.length;
  if (a && x) return `${count(a, "app", "apps")} and ${count(x, "site", "sites")}`;
  return a ? count(a, "app", "apps") : count(x, "site", "sites");
}

/** Short state of the split tunnel, as the settings rows show it. */
export function splitLabel(s: SplitTunnel): string {
  if (s.mode === "off" || (!s.apps.length && !s.sites.length)) return "Off";
  return s.mode === "bypass" ? `Excludes ${splitItems(s)}` : `Only ${splitItems(s)}`;
}

/** What SHADOW sends through the server, or null for the default (everything). */
export function routingLabel(o: ShadowOptions): string | null {
  if (o.encrypt === "dns") return "DNS only";
  const split = splitLabel(o.split);
  const parts = [o.encrypt === "traffic" && "Traffic only", split !== "Off" && split].filter(Boolean);
  return parts.length ? parts.join(" · ") : null;
}

/** A program name as the engine matches it ("C:\\Games\\Foo" -> "Foo.exe"), or null. */
export function normalizeApp(input: string): string | null {
  let s = input.trim().replace(/^"+|"+$/g, "");
  s = s.split(/[\\/]/).pop() ?? "";
  s = s.trim();
  if (!s) return null;
  if (!/\.exe$/i.test(s)) s += ".exe";
  if (s.length < 5 || s.length > 100 || /[\\/:*?"<>|\u0000-\u001f]/.test(s)) return null;
  return s;
}

/** A site entry as the engine stores it: lower-case domain, or address/prefix. */
export function normalizeSite(input: string): string | null {
  let s = input.trim().toLowerCase();
  s = s.replace(/^[a-z][a-z0-9+.-]*:\/\//, "");
  const cidr = /^\[?([0-9a-f:.]+)\]?(\/\d{1,3})?$/.exec(s);
  if (cidr && (cidr[1].includes(":") || /^\d+\.\d+\.\d+\.\d+$/.test(cidr[1]))) {
    if (!cidr[1].includes(":") && cidr[1].split(".").some((n) => Number(n) > 255)) return null;
    return cidr[1] + (cidr[2] ?? "");
  }
  s = s.split(/[/?#]/)[0].replace(/:\d+$/, "").replace(/^(\*|www)\./, "").replace(/\.$/, "");
  const label = /^(?!-)[a-z0-9-]{1,63}(?<!-)$/;
  if (s.length > 253 || !s.includes(".") || !s.split(".").every((l) => label.test(l))) return null;
  return s;
}