// Dev-only fake backend so the UI can be previewed in a plain browser with
// `npm run dev`. Loaded from main.tsx only when import.meta.env.DEV is set and
// Tauri is absent, so it never ships in a release build.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { AppSettings, Bootstrap, DiagnosticsReport, ProviderInfo, ServerProfile, Status, UpdateInfo } from "../lib/types";

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
  lotus: { provider: "cloudflare", transport: "doh", block_trackers: true, block_malware: false, block_plain_dns: false, resume_on_boot: true, record_usage: true },
  shadow: { kill_switch: true, allow_lan: true, auto_reconnect: true, resume_on_boot: true, ipv6: "tunnel", dns_provider: "cloudflare", block_trackers: true, quic: "auto", record_usage: true, encrypt: "all", split: { mode: "off", apps: [], sites: [] } },
  launch_at_startup: true,
  connect_on_launch: false,
  close_to_tray: true,
  disconnect_on_quit: false,
  motion: "system",
  auto_update: true,
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
  /** A newer version is published (found on the next check). */
  updateAvailable?: boolean;
  /** Start with the update already downloaded. */
  updateReady?: boolean;
  upTotal?: number;
  downTotal?: number;
  motion?: AppSettings["motion"];
  /** SHADOW with a split tunnel (adds the Routing row). */
  split?: boolean;
}

let update: UpdateInfo = { phase: "up_to_date", current: "1.1.0", version: null, notes: null, progress: null, error: null, checked_at: Date.now() - 42 * 60_000 };
let updateAvailable = false;

function pushUpdate(patch: Partial<UpdateInfo>) {
  update = { ...update, ...patch };
  void emit("lotus://update", update);
}

const NOTES = ["New SHADOW mark on the switch.", "Automatic updates."].join("\n");

async function checkUpdate() {
  pushUpdate({ phase: "checking", error: null });
  await new Promise((r) => setTimeout(r, 900));
  if (!updateAvailable) {
    pushUpdate({ phase: "up_to_date", checked_at: Date.now() });
    return update;
  }
  pushUpdate({ phase: "available", version: "1.2.0", notes: NOTES, checked_at: Date.now() });
  if (settings.auto_update) void downloadUpdate();
  return update;
}

async function downloadUpdate() {
  for (let p = 0; p <= 1.001; p += 0.1) {
    pushUpdate({ phase: "downloading", progress: p });
    await new Promise((r) => setTimeout(r, 180));
  }
  pushUpdate({ phase: "ready", progress: 1 });
}

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
/** Set window.__LOTUS_MOCK_FAIL__ = "shadow" (or "lotus") to make that mode fail to connect. */
const failing = () => (window as unknown as { __LOTUS_MOCK_FAIL__?: string }).__LOTUS_MOCK_FAIL__;

/** Mirrors the app: the other mode stays on until the new one connects, and comes back if it fails. */
async function connect() {
  const target = settings.mode;
  const prev = status.state === "connected" && status.mode && status.mode !== target ? status.mode : null;
  window.clearInterval(ticker);
  push({ state: "connecting", mode: target, error: null, connected_since: null, latency_ms: null, traffic: { up_rate: 0, down_rate: 0, up_total: 0, down_total: 0 } });
  await wait(1300);
  if (failing() === target) {
    push({ state: "error", error: { code: "server_unreachable", message: "The server didn't answer." } });
    if (!prev) return;
    await wait(500);
    settings = { ...settings, mode: prev };
    void emit("lotus://settings", settings);
    push({ state: "connecting", mode: prev, error: null });
    await wait(900);
    settings = { ...settings, mode: prev };
    goLive(0);
    const name = (m: string) => m.toUpperCase();
    throw { code: "server_unreachable", message: `${name(target)} couldn't connect, so ${name(prev)} is back on. The server didn't answer.` };
  }
  goLive(0);
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
    latency_ms: mode === "shadow" ? 38 : 14,
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
    if (status.state !== "connected") return;
    const shadow = status.mode === "shadow";
    // LOTUS only carries encrypted DNS: a few hundred bytes a second.
    const up = shadow ? Math.round(4_000 + Math.random() * 40_000) : Math.round(Math.random() * 900);
    const down = shadow ? Math.round(60_000 + Math.random() * 2_400_000) : Math.round(Math.random() * 2_600);
    push({
      latency_ms: shadow ? 34 + Math.round(Math.random() * 8) : 11 + Math.round(Math.random() * 9),
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
        case "save_settings": {
          const old = settings;
          settings = a.settings as AppSettings;
          // A live session picks up its mode's changes at once.
          const m = status.state === "connected" ? status.mode : null;
          if (m && JSON.stringify(old[m]) !== JSON.stringify(settings[m])) {
            const keep = settings.mode;
            settings = { ...settings, mode: m };
            await connect();
            settings = { ...settings, mode: keep };
          }
          return settings;
        }
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
        case "connections": {
          if (status.mode !== "shadow" || status.state !== "connected") return { active: false, connections: [], upload_total: 0, download_total: 0 };
          const t = Math.floor(Date.now() / 1000);
          const hosts = [["github.com", 443], ["api.anthropic.com", 443], ["www.youtube.com", 443], ["i.ytimg.com", 443], ["open.spotify.com", 443], ["time.cloudflare.com", 123], ["192.168.1.254", 80], ["fonts.gstatic.com", 443], ["news.ycombinator.com", 443]] as const;
          const conns = hosts
            .filter((_, i) => (t + i * 3) % 7 !== 0)
            .map(([host, port], i) => ({ id: `${host}-${Math.floor((t + i * 3) / 7)}`, host, port, network: port === 123 ? "udp" : "tcp", route: host.startsWith("192.") ? "direct" : "proxy", upload: (i + 1) * 4_200 * ((t % 30) + 1), download: (i + 1) * 380_000 * ((t % 30) + 1), started: new Date().toISOString() }));
          return { active: true, connections: conns.sort((a, b) => b.upload + b.download - (a.upload + a.download)), upload_total: status.traffic.up_total, download_total: status.traffic.down_total };
        }
        case "verify_server":
          await new Promise((r) => setTimeout(r, 1400));
          return {
            protocol: "VLESS · REALITY · Vision over TCP 443",
            generated_at: Date.now(),
            checks: [
              { id: "handshake", title: "Authenticated handshake", outcome: "pass", summary: "Your server accepted LOTUS's key (139 ms round trip)", detail: "REALITY only completes when the server proves it holds the private key matching this profile's public key, so this is your REALITY server and not an impostor." },
              { id: "camouflage", title: "What strangers see", outcome: "pass", summary: "The real www.example.com", detail: "A visitor without your key gets www.example.com's own valid certificate (SHA-256 35664ec6ea5bdb84…), identical to the one the real site serves and HTTP 200. Nothing reveals a proxy." },
              { id: "exit", title: "Exit address", outcome: status.state === "connected" ? "pass" : "info", summary: status.state === "connected" ? "Websites see 203.0.113.10" : "Connect SHADOW through this server to check", detail: status.state === "connected" ? "That's your server's address, so traffic is leaving through it." : "" },
              { id: "encryption", title: "Encryption", outcome: "info", summary: "VLESS · REALITY · Vision over TCP 443", detail: "TLS 1.3 (REALITY): X25519 key exchange authenticated with your server's key, AES-GCM or ChaCha20 for data. XTLS-Vision avoids double-encrypting traffic that is already TLS." },
            ],
          };
        case "usage": {
          // Six weeks of plausible history, generated per session.
          const out = [];
          const now = new Date();
          for (let i = 0; i < 300; i++) {
            const d = new Date(now.getFullYear(), now.getMonth(), now.getDate() - i);
            const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
            const seed = Math.sin(i * 12.9898) * 43758.5453;
            const r = seed - Math.floor(seed);
            if (r < 0.18) continue;
            out.push({ date: key, mode: "lotus", seconds: Math.round(3600 * (2 + r * 6)), up: 0, down: 0 });
            if (r > 0.4) out.push({ date: key, mode: "shadow", server_id: "p1", server: "Frankfurt VPS", seconds: Math.round(3600 * r * 5), up: Math.round(4e7 * r), down: Math.round(1.6e9 * r * r) });
            if (r > 0.85) out.push({ date: key, mode: "shadow", server_id: "p2", server: "Home lab", seconds: Math.round(1800 * r), up: Math.round(9e6 * r), down: Math.round(2e8 * r) });
          }
          return { days: out };
        }
        case "clear_usage":
          return null;
        case "running_apps":
          return ["chrome.exe", "Discord.exe", "Spotify.exe", "steam.exe", "Code.exe", "valorant.exe"];
        case "update_status":
          return update;
        case "check_update":
          return checkUpdate();
        case "install_update":
          if (update.phase !== "ready") await downloadUpdate();
          pushUpdate({ phase: "installing" });
          window.setTimeout(() => pushUpdate({ phase: "ready" }), 2000);
          return null;
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
  if (preset.motion) settings = { ...settings, motion: preset.motion };
  if (preset.split) settings = { ...settings, shadow: { ...settings.shadow, split: { mode: "bypass", apps: ["Discord.exe"], sites: [] } } };
  updateAvailable = !!(preset.updateAvailable || preset.updateReady);
  if (preset.updateReady) update = { ...update, phase: "ready", version: "1.2.0", notes: NOTES, progress: 1, checked_at: Date.now() - 5 * 60_000 };
  if (preset.connected) {
    status = { ...status, traffic: { up_rate: 0, down_rate: 0, up_total: preset.upTotal ?? 0, down_total: preset.downTotal ?? 0 } };
    goLive(preset.sessionAgeMs ?? 0);
  }
}
