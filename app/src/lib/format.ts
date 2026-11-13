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
