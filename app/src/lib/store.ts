import { useSyncExternalStore } from "react";
import { listen } from "@tauri-apps/api/event";
import { toast as sonner } from "sonner";
import { api, errorText } from "./api";
import type { AppSettings, Mode, ProviderInfo, ServerProfile, Status, UpdateInfo } from "./types";

/** Summary of the most recent session in this app run, shown while off. */
export interface LastSession {
  mode: Mode;
  bytes: number;
  ms: number;
}

export interface State {
  ready: boolean;
  settings: AppSettings | null;
  profiles: ServerProfile[];
  status: Status | null;
  service: { connected: boolean; message: string | null };
  providers: ProviderInfo[];
  version: string;
  engineVersion: string;
  storeWarning: string | null;
  lastSession: LastSession | null;
  update: UpdateInfo | null;
}

let state: State = {
  ready: false,
  settings: null,
  profiles: [],
  status: null,
  service: { connected: false, message: null },
  providers: [],
  version: "",
  engineVersion: "",
  storeWarning: null,
  lastSession: null,
  update: null,
};

const listeners = new Set<() => void>();

const isLive = (s: Status | null) => !!s && (s.state === "connected" || s.state === "reconnecting");

/** When a live session ends, keep its totals so Home can show them afterwards. */
function sessionEnded(prev: Status | null, next: Status | null): Partial<State> {
  if (!isLive(prev) || isLive(next) || !prev?.mode || !prev.connected_since) return {};
  return {
    lastSession: {
      mode: prev.mode,
      bytes: prev.traffic.up_total + prev.traffic.down_total,
      ms: Math.max(0, Date.now() - prev.connected_since),
    },
  };
}

export function setState(patch: Partial<State>) {
  if ("status" in patch && patch.status !== state.status) patch = { ...sessionEnded(state.status, patch.status ?? null), ...patch };
  state = { ...state, ...patch };
  listeners.forEach((l) => l());
}

export function getState() {
  return state;
}

export function useStore<T>(sel: (s: State) => T): T {
  return useSyncExternalStore(
    (cb) => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    () => sel(state),
  );
}

export function toast(text: string, tone: "info" | "error" = "info") {
  if (tone === "error") sonner.error(text, { duration: 5200 });
  else sonner(text, { duration: 2600 });
}

/** Run the update installer; LOTUS exits and relaunches when it finishes. */
export async function installUpdate() {
  try {
    await api.installUpdate();
  } catch (e) {
    toast(errorText(e), "error");
  }
}

// Announce a downloaded update once per version while the window is open.
let announced: string | null = null;
function announce(u: UpdateInfo) {
  if (u.phase !== "ready" || !u.version || announced === u.version) return;
  announced = u.version;
  sonner(`LOTUS ${u.version} is ready to install`, {
    duration: 12000,
    action: { label: "Restart", onClick: () => void installUpdate() },
  });
}

function setUpdate(u: UpdateInfo) {
  setState({ update: u });
  announce(u);
}

export async function init() {
  await listen<Status>("lotus://status", (e) => {
    // Drop out-of-order pushes.
    if (!state.status || e.payload.seq >= state.status.seq || e.payload.seq < 3) setState({ status: e.payload });
  });
  await listen<{ connected: boolean; message: string | null }>("lotus://service", (e) => {
    setState({ service: e.payload, ...(e.payload.connected ? {} : { status: null }) });
  });
  await listen<AppSettings>("lotus://settings", (e) => setState({ settings: e.payload }));
  await listen<UpdateInfo>("lotus://update", (e) => setUpdate(e.payload));
  const b = await api.bootstrap();
  setState({
    ready: true,
    settings: b.settings,
    profiles: b.profiles,
    status: b.status,
    service: { connected: b.service_connected, message: null },
    providers: b.providers,
    version: b.version,
    engineVersion: b.engine_version,
    storeWarning: b.store_warning,
  });
  api.updateStatus().then(setUpdate, () => {});
}

export async function updateSettings(fn: (s: AppSettings) => AppSettings) {
  const cur = state.settings;
  if (!cur) return;
  const next = fn(structuredClone(cur));
  setState({ settings: next });
  try {
    setState({ settings: await api.saveSettings(next) });
  } catch (e) {
    setState({ settings: cur });
    throw e;
  }
}
