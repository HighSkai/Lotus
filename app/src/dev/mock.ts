// Dev-only fake backend so the UI can be previewed in a plain browser with
// `npm run dev`. Loaded from main.tsx only when import.meta.env.DEV is set and
// Tauri is absent, so it never ships in a release build.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { AppSettings, Bootstrap, DiagnosticsReport, ProviderInfo, ServerProfile, Status } from "../lib/types";

const providers: ProviderInfo[] = [
  { id: "cloudflare", name: "Cloudflare", doh3: true, dot: true, malware_variant: true, note: "Fastest worldwide" },
  { id: "quad9", name: "Quad9", doh3: false, dot: true, malware_variant: true, note: "Non-profit, blocks malware" },
  { id: "mullvad", name: "Mullvad", doh3: false, dot: true, malware_variant: false, note: "No logging" },
  { id: "adguard", name: "AdGuard", doh3: true, dot: true, malware_variant: true, note: "Blocks ads" },
  { id: "google", name: "Google", doh3: true, dot: true, malware_variant: false, note: "Large anycast network" },
];

let profiles: ServerProfile[] = [
  {
    id: "p1",
    name: "Frankfurt VPS",
    created_at: 0,
    protocol: { type: "vless", server: "203.0.113.10", port: 443, uuid: "00000000-0000-4000-8000-000000000000", flow: "xtls-rprx-vision", tls: { server_name: "www.example.com", fingerprint: "chrome", reality: { public_key: "pk", short_id: "ab" } }, transport: { type: "tcp" } },
  },
  { id: "p2", name: "Home lab", created_at: 0, protocol: { type: "hysteria2", server: "198.51.100.7", port: 8443, password: "x", tls: {} } },
];

let settings: AppSettings = {
  mode: "lotus",
  selected_profile: "p1",
  lotus: { provider: "cloudflare", transport: "doh", block_trackers: true, block_malware: false, block_plain_dns: false, resume_on_boot: true },
  shadow: { kill_switch: true, allow_lan: true, auto_reconnect: true, resume_on_boot: true, ipv6: "tunnel", dns_provider: "cloudflare", block_trackers: true, quic: "auto" },
  launch_at_startup: true,
  connect_on_launch: false,
  close_to_tray: true,
  disconnect_on_quit: false,
  motion: "system",
};

let seq = 1;
let status: Status = {
  state: "disconnected",
  mode: null,
  connected_since: null,
  profile_id: null,
  profile_name: null,
  protocol_label: null,
  exit: null,
  latency_ms: null,
  traffic: { up_rate: 0, down_rate: 0, up_total: 0, down_total: 0 },
  error: null,
  protections: [],
  kill_switch_engaged: false,
  service_version: "1.0.2",
  engine_version: "1.12.0",
  seq,
};
let ticker = 0;

function push(patch: Partial<Status>) {
  status = { ...status, ...patch, seq: ++seq };
  void emit("lotus://status", status);
}

/** Optional preset for screenshots: set window.__LOTUS_MOCK__ before the app loads. */
interface Preset {
  mode?: AppSettings["mode"];
  connected?: boolean;
  sessionAgeMs?: number;
  upTotal?: number;
  downTotal?: number;
}

function connect() {
  push({ state: "connecting", mode: settings.mode, error: null });
  window.setTimeout(() => goLive(0), 1300);
}

function goLive(ageMs: number) {
  const mode = settings.mode;
  const p = profiles.find((x) => x.id === settings.selected_profile);
  push({
    state: "connected",
    mode,
    error: null,
    connected_since: Date.now() - ageMs,
    profile_id: mode === "shadow" ? p?.id ?? null : null,
    profile_name: mode === "shadow" ? p?.name ?? null : null,
    protocol_label: mode === "shadow" ? "VLESS · REALITY" : null,
    exit: mode === "shadow" ? { ipv4: "203.0.113.10", ipv6: null, country: "DE", colo: "FRA" } : { ipv4: "192.0.2.44", ipv6: null, country: "US", colo: null },
    latency_ms: mode === "shadow" ? 38 : null,
    protections:
      mode === "lotus"
        ? [
            { id: "encrypted_dns", label: "Encrypted DNS", detail: "Cloudflare · DNS-over-HTTPS", active: true },
            { id: "dns_leak", label: "DNS leak protection", detail: "All adapters use LOTUS", active: true },
            { id: "trackers", label: "Tracker blocking", detail: "Known ad & tracking domains", active: settings.lotus.block_trackers },
            { id: "malware", label: "Malicious-domain filter", detail: "Off", active: false },
          ]
        : [],
  });
  window.clearInterval(ticker);
  ticker = window.setInterval(() => {
    if (status.mode !== "shadow") return;
    const up = Math.round(4_000 + Math.random() * 40_000);
    const down = Math.round(60_000 + Math.random() * 2_400_000);
    push({
      latency_ms: 34 + Math.round(Math.random() * 8),
      traffic: { up_rate: up, down_rate: down, up_total: status.traffic.up_total + up, down_total: status.traffic.down_total + down },
    });
  }, 1000);
}

function disconnect() {
  window.clearInterval(ticker);
  push({ state: "disconnecting" });
  window.setTimeout(() => {
    push({ state: "disconnected", connected_since: null, latency_ms: null, exit: null, traffic: { up_rate: 0, down_rate: 0, up_total: 0, down_total: 0 }, protections: [] });
  }, 600);
}

const diagnostics = (): DiagnosticsReport => ({
  mode: status.state === "connected" ? status.mode : null,
  state: status.state,
  generated_at: Date.now(),
  checks: [
    { id: "dns", title: "DNS resolver", outcome: "pass", summary: "Cloudflare, encrypted", detail: "Every test lookup was answered by the encrypted resolver." },
    { id: "v4", title: "IPv4", outcome: "pass", summary: "203.0.113.10 (Germany)", detail: "" },
    { id: "v6", title: "IPv6", outcome: "warn", summary: "Not tunnelled", detail: "Your server has no IPv6, so IPv6 is blocked." },
    { id: "plain_dns", title: "Hard-coded DNS", outcome: "info", summary: "Not blocked (setting off)", detail: "" },
  ],
});

export async function installMock() {
  // Tauri freezes Object.prototype in the real app; mirror that so preview catches the same breakage.
  Object.freeze(Object.prototype);
  mockWindows("main");
  mockIPC(
    async (cmd, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "bootstrap":
          return { settings, profiles, status, service_connected: true, providers, version: "1.0.2", engine_version: "1.12.0", store_warning: null } satisfies Bootstrap;
        case "save_settings":
          settings = a.settings as AppSettings;
          return settings;
        case "set_mode":
          settings = { ...settings, mode: a.mode as AppSettings["mode"] };
          return settings;
        case "select_profile":
          settings = { ...settings, selected_profile: a.id as string };
          void emit("lotus://settings", settings);
          return settings;
        case "connect":
          return connect();
        case "disconnect":
          return disconnect();
        case "delete_profile":
          profiles = profiles.filter((p) => p.id !== a.id);
          return profiles;
        case "save_profile":
          profiles = [...profiles.filter((p) => p.id !== (a.profile as ServerProfile).id), { ...(a.profile as ServerProfile), id: (a.profile as ServerProfile).id || `p${Date.now()}` }];
          return profiles;
        case "test_profile":
          await new Promise((r) => setTimeout(r, 700));
          return { ok: true, latency_ms: 41, tcp_ms: 18, message: "Handshake OK" };
        case "import_text":
        case "import_clipboard":
          return { added: [], issues: [{ source: "line 1", message: "That doesn't look like a server link." }], profiles, selected_profile: settings.selected_profile };
        case "diagnostics":
          await new Promise((r) => setTimeout(r, 900));
          return diagnostics();
        case "speed_test":
          await new Promise((r) => setTimeout(r, 1500));
          return { download_mbps: 412.6, upload_mbps: 88.1, latency_ms: 36, bytes_down: 40_000_000, bytes_up: 9_000_000, message: "Measured with speed.cloudflare.com." };
        case "logs":
          return [];
        case "server_script":
          return "#!/bin/bash";
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
  const preset = ((window as unknown as { __LOTUS_MOCK__?: Preset }).__LOTUS_MOCK__ ?? {}) as Preset;
  if (preset.mode) settings = { ...settings, mode: preset.mode };
  if (preset.connected) {
    status = { ...status, traffic: { up_rate: 0, down_rate: 0, up_total: preset.upTotal ?? 0, down_total: preset.downTotal ?? 0 } };
    goLive(preset.sessionAgeMs ?? 0);
  }
}
