import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  Bootstrap,
  DiagnosticsReport,
  ErrorInfo,
  ImportResult,
  Mode,
  ServerProfile,
  SpeedResult,
  TestResult,
  UpdateInfo,
  ConnectionsSnapshot,
  VerifyReport,
} from "./types";

export function errorText(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) return String((e as ErrorInfo).message);
  return String(e);
}

export const api = {
  bootstrap: () => invoke<Bootstrap>("bootstrap"),
  appReady: () => invoke<void>("app_ready"),
  saveSettings: (settings: AppSettings) => invoke<AppSettings>("save_settings", { settings }),
  setMode: (mode: Mode) => invoke<AppSettings>("set_mode", { mode }),
  connect: () => invoke<void>("connect"),
  disconnect: () => invoke<void>("disconnect"),
  importText: (text: string) => invoke<ImportResult>("import_text", { text }),
  importClipboard: () => invoke<ImportResult>("import_clipboard"),
  saveProfile: (profile: ServerProfile) => invoke<ServerProfile[]>("save_profile", { profile }),
  deleteProfile: (id: string) => invoke<ServerProfile[]>("delete_profile", { id }),
  selectProfile: (id: string) => invoke<AppSettings>("select_profile", { id }),
  testProfile: (id: string) => invoke<TestResult>("test_profile", { id }),
  copyShareLink: (id: string) => invoke<void>("copy_share_link", { id }),
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  diagnostics: () => invoke<DiagnosticsReport>("diagnostics"),
  speedTest: () => invoke<SpeedResult>("speed_test"),
  connections: () => invoke<ConnectionsSnapshot>("connections"),
  verifyServer: (id: string) => invoke<VerifyReport>("verify_server", { id }),
  logs: () => invoke<string[]>("logs"),
  serverScript: () => invoke<string>("server_script"),
  saveServerScript: () => invoke<string | null>("save_server_script"),
  repairService: () => invoke<void>("repair_service"),
  closeWindow: () => invoke<void>("close_window"),
  quit: () => invoke<void>("quit"),
  updateStatus: () => invoke<UpdateInfo>("update_status"),
  checkUpdate: () => invoke<UpdateInfo>("check_update"),
  installUpdate: () => invoke<void>("install_update"),
};
