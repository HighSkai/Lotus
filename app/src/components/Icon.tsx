// Hand-drawn 24px stroke icons (no icon font, no network).
const P: Record<string, string[]> = {
  gear: [
    "M12 15.2a3.2 3.2 0 1 0 0-6.4 3.2 3.2 0 0 0 0 6.4Z",
    "M19.4 13.5a7.6 7.6 0 0 0 0-3l2-1.5-2-3.4-2.3.9a7.5 7.5 0 0 0-2.6-1.5L14.1 2.6h-4.2l-.4 2.4A7.5 7.5 0 0 0 6.9 6.5l-2.3-.9-2 3.4 2 1.5a7.6 7.6 0 0 0 0 3l-2 1.5 2 3.4 2.3-.9a7.5 7.5 0 0 0 2.6 1.5l.4 2.4h4.2l.4-2.4a7.5 7.5 0 0 0 2.6-1.5l2.3.9 2-3.4-2-1.5Z",
  ],
  back: ["M15 5l-7 7 7 7"],
  chevron: ["M9 5l7 7-7 7"],
  close: ["M6 6l12 12M18 6L6 18"],
  minus: ["M5 12h14"],
  plus: ["M12 5v14M5 12h14"],
  server: ["M4 5.5A1.5 1.5 0 0 1 5.5 4h13A1.5 1.5 0 0 1 20 5.5v4A1.5 1.5 0 0 1 18.5 11h-13A1.5 1.5 0 0 1 4 9.5v-4Z", "M4 14.5A1.5 1.5 0 0 1 5.5 13h13a1.5 1.5 0 0 1 1.5 1.5v4a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 18.5v-4Z", "M8 7.5h.01M8 16.5h.01"],
  shield: ["M12 3l7.5 3v5.5c0 4.6-3.2 8.3-7.5 9.5-4.3-1.2-7.5-4.9-7.5-9.5V6L12 3Z"],
  shieldCheck: ["M12 3l7.5 3v5.5c0 4.6-3.2 8.3-7.5 9.5-4.3-1.2-7.5-4.9-7.5-9.5V6L12 3Z", "M8.8 12.2l2.2 2.2 4.4-4.6"],
  globe: ["M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Z", "M3.5 9h17M3.5 15h17", "M12 3c2.3 2.4 3.5 5.4 3.5 9s-1.2 6.6-3.5 9c-2.3-2.4-3.5-5.4-3.5-9S9.7 5.4 12 3Z"],
  clock: ["M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Z", "M12 7.5V12l3 2"],
  pulse: ["M3 12h4l2.5-6 5 12 2.5-6H21"],
  up: ["M12 19V5M6 11l6-6 6 6"],
  down: ["M12 5v14M6 13l6 6 6-6"],
  check: ["M5 12.5l4.5 4.5L19 7.5"],
  alert: ["M12 4l9 16H3l9-16Z", "M12 10v4.5M12 17.5h.01"],
  info: ["M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Z", "M12 11v5.5M12 7.8h.01"],
  copy: ["M9 9h10.5v10.5H9z", "M5 15V5h10"],
  trash: ["M4.5 7h15", "M9.5 7V4.5h5V7", "M6.5 7l1 13h9l1-13"],
  edit: ["M4 20h4L19 9l-4-4L4 16v4Z", "M13.5 6.5l4 4"],
  clipboard: ["M8.5 4.5h7v3h-7z", "M8.5 5.5H6v15h12v-15h-2.5"],
  file: ["M6 3h8l4 4v14H6z", "M14 3v4h4"],
  wand: ["M4 20l10-10", "M15 4v3M13.5 5.5h3M19 9v2.5M17.75 10.25h2.5M10 3.5v2M9 4.5h2"],
  lock: ["M6 11h12v9H6z", "M8.5 11V8a3.5 3.5 0 1 1 7 0v3"],
  zap: ["M13 3L5 13.5h6L10 21l8-10.5h-6L13 3Z"],
  refresh: ["M20 12a8 8 0 1 1-2.3-5.6", "M20 4v4.5h-4.5"],
  power: ["M12 3.5v8", "M7 6.3a7.5 7.5 0 1 0 10 0"],
  wrench: ["M14.5 6.5a4 4 0 0 0 5 5l-8 8a2.1 2.1 0 0 1-3-3l8-8a4 4 0 0 0-2-2Z"],
  sliders: ["M5 6h9M18 6h1M5 12h3M12 12h7M5 18h11M20 18h-1", "M16 4v4M10 10v4M18 16v4"],
  speed: ["M4.5 17a8.5 8.5 0 1 1 15 0", "M12 13l4-4"],
  list: ["M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01"],
  external: ["M14 4h6v6", "M20 4l-9 9", "M18 14v6H4V6h6"],
  eye: ["M2.5 12s3.5-6.5 9.5-6.5 9.5 6.5 9.5 6.5-3.5 6.5-9.5 6.5S2.5 12 2.5 12Z", "M12 14.8a2.8 2.8 0 1 0 0-5.6 2.8 2.8 0 0 0 0 5.6Z"],
  play: ["M8 5.5v13l10-6.5-10-6.5Z"],
  cpu: ["M7 7h10v10H7z", "M10 3v4M14 3v4M10 17v4M14 17v4M3 10h4M3 14h4M17 10h4M17 14h4"],
  moon: ["M20 14.5A8.5 8.5 0 0 1 9.5 4a8.5 8.5 0 1 0 10.5 10.5Z"],
};

export function Icon({ name, className, strokeWidth = 1.75 }: { name: keyof typeof P | string; className?: string; strokeWidth?: number }) {
  const paths = P[name] ?? P.info;
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {paths.map((d, i) => (
        <path key={i} d={d} />
      ))}
    </svg>
  );
}
