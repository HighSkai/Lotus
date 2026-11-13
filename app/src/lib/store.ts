import { useSyncExternalStore } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import type { AppSettings, ProviderInfo, ServerProfile, Status } from "./types";

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
  toast: { id: number; text: string; tone: "info" | "error" } | null;
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
  toast: null,
};

const listeners = new Set<() => void>();

export function setState(patch: Partial<State>) {
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

let toastSeq = 0;
export function toast(text: string, tone: "info" | "error" = "info") {
  const id = ++toastSeq;
  setState({ toast: { id, text, tone } });
  window.setTimeout(() => {
    if (state.toast?.id === id) setState({ toast: null });
  }, tone === "error" ? 5200 : 2600);
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
