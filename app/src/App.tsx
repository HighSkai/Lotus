import { useEffect, useLayoutEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Icon } from "./components/Icon";
import { api, errorText } from "./lib/api";
import { setState, toast, useStore } from "./lib/store";
import type { Mode } from "./lib/types";
import { Home, visualFor } from "./screens/Home";
import { About, AdvancedSettings, ConnectionSettings, DnsSettings, GeneralSettings, KillSwitchSettings, LogsPage, SettingsIndex } from "./screens/Settings";
import { AddServer, Servers, ServerSetup } from "./screens/Servers";
import { ServerEditor } from "./screens/ServerEditor";
import { Diagnostics, SpeedTest } from "./screens/Tools";

export type Route =
  | { name: "settings" | "general" | "dns" | "connection" | "killswitch" | "servers" | "add-server" | "setup" | "diagnostics" | "speed" | "advanced" | "about" | "logs" }
  | { name: "edit-server"; id?: string };

export interface Nav {
  push: (r: Route) => void;
  back: () => void;
  replace: (r: Route) => void;
  home: () => void;
}

function Screen({ route, nav }: { route: Route; nav: Nav }) {
  switch (route.name) {
    case "settings":
      return <SettingsIndex nav={nav} />;
    case "general":
      return <GeneralSettings nav={nav} />;
    case "dns":
      return <DnsSettings nav={nav} />;
    case "connection":
      return <ConnectionSettings nav={nav} />;
    case "killswitch":
      return <KillSwitchSettings nav={nav} />;
    case "servers":
      return <Servers nav={nav} />;
    case "add-server":
      return <AddServer nav={nav} />;
    case "edit-server":
      return <ServerEditor nav={nav} id={route.id} />;
    case "setup":
      return <ServerSetup nav={nav} />;
    case "diagnostics":
      return <Diagnostics nav={nav} />;
    case "speed":
      return <SpeedTest nav={nav} />;
    case "advanced":
      return <AdvancedSettings nav={nav} />;
    case "logs":
      return <LogsPage nav={nav} />;
    case "about":
      return <About nav={nav} />;
  }
}

function Backdrop({ on }: { on: boolean }) {
  return (
    <div className="backdrop" data-on={on} aria-hidden="true">
      <div className="orb orb-1" />
      <div className="orb orb-2" />
      <div className="orb orb-3" />
      <div className="halo" />
      <div className="grain" />
    </div>
  );
}

export function App() {
  const ready = useStore((s) => s.ready);
  const settings = useStore((s) => s.settings);
  const status = useStore((s) => s.status);
  const service = useStore((s) => s.service);
  const toastState = useStore((s) => s.toast);
  const [stack, setStack] = useState<Route[]>([]);
  const mode: Mode = settings?.mode ?? "lotus";

  const nav: Nav = {
    push: (r) => setStack((s) => [...s, r]),
    back: () => setStack((s) => s.slice(0, -1)),
    replace: (r) => setStack((s) => [...s.slice(0, -1), r]),
    home: () => setStack([]),
  };

  useLayoutEffect(() => {
    const root = document.documentElement;
    root.dataset.mode = mode;
    root.dataset.motion = settings?.motion ?? "system";
  }, [mode, settings?.motion]);

  useEffect(() => {
    if (ready) requestAnimationFrame(() => api.appReady());
  }, [ready]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setStack((s) => s.slice(0, -1));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  async function switchMode(m: Mode) {
    if (!settings || m === settings.mode) return;
    setState({ settings: { ...settings, mode: m } });
    try {
      setState({ settings: await api.setMode(m) });
    } catch (e) {
      toast(errorText(e), "error");
    }
  }

  if (!ready || !settings) return <Backdrop on={false} />;
  const on = visualFor(status, mode, service.connected, false) === "on";
  const top = stack[stack.length - 1];

  return (
    <>
      <Backdrop on={on && !top} />
      <div className="shell">
        <header className="titlebar" data-tauri-drag-region>
          <div className="left">
            <button className="icon-btn" aria-label="Settings" title="Settings" onClick={() => (top?.name === "settings" ? nav.back() : setStack([{ name: "settings" }]))}>
              <Icon name="gear" />
            </button>
          </div>
          <div className="modes" data-mode={mode} role="group" aria-label="Mode">
            <span className="thumb" />
            <button aria-pressed={mode === "lotus"} onClick={() => switchMode("lotus")} title="Private DNS, same IP">
              LOTUS
            </button>
            <button aria-pressed={mode === "shadow"} onClick={() => switchMode("shadow")} title="Full tunnel through your server">
              SHADOW
            </button>
          </div>
          <div className="right">
            <button className="icon-btn" aria-label="Minimize" onClick={() => getCurrentWindow().minimize()}>
              <Icon name="minus" />
            </button>
            <button className="icon-btn close" aria-label="Close" onClick={() => api.closeWindow()}>
              <Icon name="close" />
            </button>
          </div>
        </header>
        <main className="view">
          <Home nav={nav} />
          {top && <Screen key={stack.length + top.name} route={top} nav={nav} />}
        </main>
      </div>
      {toastState && (
        <div key={toastState.id} className={`toast ${toastState.tone}`} role="status">
          {toastState.text}
        </div>
      )}
    </>
  );
}
