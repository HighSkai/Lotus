// Mirrors the Rust types in crates/lotus-core (serde field names).

export type Mode = "lotus" | "shadow";
export type ConnState = "disconnected" | "connecting" | "connected" | "reconnecting" | "disconnecting" | "error";
export type DnsProviderId = "cloudflare" | "quad9" | "google" | "mullvad" | "adguard" | "custom";
export type DnsTransport = "doh" | "doh3" | "dot";

export interface TlsConfig {
  server_name?: string;
  alpn?: string[];
  fingerprint?: string;
  reality?: { public_key: string; short_id: string };
  certificate?: string;
  public_key_sha256?: string[];
}

export type Transport =
  | { type: "tcp" }
  | { type: "ws"; path: string; host?: string }
  | { type: "grpc"; service_name: string }
  | { type: "http"; path: string; host?: string[] }
  | { type: "httpupgrade"; path: string; host?: string };

export type Protocol =
  | { type: "vless"; server: string; port: number; uuid: string; flow?: string; tls?: TlsConfig; transport: Transport }
  | { type: "trojan"; server: string; port: number; password: string; tls: TlsConfig; transport: Transport }
  | { type: "shadowsocks"; server: string; port: number; method: string; password: string }
  | { type: "hysteria2"; server: string; port: number; password: string; obfs?: { kind: string; password: string }; tls: TlsConfig; up_mbps?: number; down_mbps?: number }
  | { type: "tuic"; server: string; port: number; uuid: string; password: string; congestion_control: string; udp_relay_mode?: string; tls: TlsConfig }
  | { type: "anytls"; server: string; port: number; password: string; tls: TlsConfig }
  | { type: "naive"; server: string; port: number; username: string; password: string; tls: TlsConfig; quic: boolean }
  | {
      type: "wireguard";
      server: string;
      port: number;
      private_key: string;
      peer_public_key: string;
      pre_shared_key?: string;
      local_address: string[];
      mtu?: number;
      reserved?: [number, number, number];
      keepalive?: number;
    };

export interface ServerProfile {
  id: string;
  name: string;
  protocol: Protocol;
  created_at: number;
}

export interface LotusOptions {
  provider: DnsProviderId;
  transport: DnsTransport;
  custom?: { url: string; ip: string };
  block_trackers: boolean;
  block_malware: boolean;
  block_plain_dns: boolean;
  resume_on_boot: boolean;
  record_usage: boolean;
}

export interface ShadowOptions {
  kill_switch: boolean;
  allow_lan: boolean;
  auto_reconnect: boolean;
  resume_on_boot: boolean;
  ipv6: "tunnel" | "disable";
  dns_provider: DnsProviderId;
  block_trackers: boolean;
  quic: "auto" | "block" | "allow";
  mtu?: number;
  record_usage: boolean;
}

export interface AppSettings {
  mode: Mode;
  selected_profile: string | null;
  lotus: LotusOptions;
  shadow: ShadowOptions;
  launch_at_startup: boolean;
  connect_on_launch: boolean;
  close_to_tray: boolean;
  disconnect_on_quit: boolean;
  motion: "system" | "reduce" | "full";
  auto_update: boolean;
}

export interface ErrorInfo {
  code: string;
  message: string;
}

export interface Protection {
  id: string;
  label: string;
  detail: string;
  active: boolean;
}

export interface Status {
  state: ConnState;
  mode: Mode | null;
  connected_since: number | null;
  profile_id: string | null;
  profile_name: string | null;
  protocol_label: string | null;
  exit: { ipv4: string | null; ipv6: string | null; country: string | null; colo: string | null } | null;
  latency_ms: number | null;
  traffic: { up_rate: number; down_rate: number; up_total: number; down_total: number };
  error: ErrorInfo | null;
  protections: Protection[];
  kill_switch_engaged: boolean;
  service_version: string;
  engine_version: string;
  seq: number;
}

export interface ProviderInfo {
  id: DnsProviderId;
  name: string;
  doh3: boolean;
  dot: boolean;
  malware_variant: boolean;
  note: string;
}

export interface Bootstrap {
  settings: AppSettings;
  profiles: ServerProfile[];
  status: Status | null;
  service_connected: boolean;
  providers: ProviderInfo[];
  version: string;
  engine_version: string;
  store_warning: string | null;
}

export interface TestResult {
  ok: boolean;
  latency_ms: number | null;
  tcp_ms: number | null;
  message: string;
}

export interface SpeedResult {
  download_mbps: number | null;
  upload_mbps: number | null;
  latency_ms: number | null;
  bytes_down: number;
  bytes_up: number;
  message: string;
}

export interface Check {
  id: string;
  title: string;
  outcome: "pass" | "warn" | "fail" | "info";
  summary: string;
  detail: string;
}

export interface DiagnosticsReport {
  mode: Mode | null;
  state: ConnState;
  checks: Check[];
  generated_at: number;
}

export interface ImportResult {
  added: ServerProfile[];
  issues: { source: string; message: string }[];
  profiles: ServerProfile[];
  selected_profile: string | null;
}

export type UpdatePhase = "idle" | "checking" | "up_to_date" | "available" | "downloading" | "ready" | "installing" | "error";

export interface UpdateInfo {
  phase: UpdatePhase;
  current: string;
  version: string | null;
  notes: string | null;
  progress: number | null;
  error: string | null;
  checked_at: number | null;
}

export interface ConnInfo {
  id: string;
  host: string;
  port: number;
  network: string;
  route: string;
  upload: number;
  download: number;
  started: string;
}

export interface ConnectionsSnapshot {
  active: boolean;
  connections: ConnInfo[];
  upload_total: number;
  download_total: number;
}

export interface UsageDay {
  date: string;
  mode: Mode;
  server_id?: string;
  server?: string;
  seconds: number;
  up: number;
  down: number;
}

export interface VerifyReport {
  protocol: string;
  checks: Check[];
  generated_at: number;
}
