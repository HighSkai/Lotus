import { lazy, Suspense, useEffect, useLayoutEffect, useState, type ComponentType } from "react";
import { flushSync } from "react-dom";
import { AnimatePresence, MotionConfig, motion } from "motion/react";
import { Minus, Settings, X } from "lucide-react";
import { IconButton } from "@/components/Controls";
import { Toaster } from "@/components/ui/sonner";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { TooltipProvider } from "@/components/ui/tooltip";
import { api, errorText } from "@/lib/api";
import { easeOut, spring } from "@/lib/motion";
import { setState, toast, useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import { minimizeWindow, startWindowDrag } from "@/lib/window";
import type { Mode } from "@/lib/types";
import { Home, visualFor } from "./screens/Home";

// Everything except Home loads on demand, then gets prefetched once the app is idle.
const loaders = {
  settings: () => import("./screens/Settings"),
  servers: () => import("./screens/Servers"),
  editor: () => import("./screens/ServerEditor"),
  tools: () => import("./screens/Tools"),
  updates: () => import("./screens/Updates"),
  console: () => import("./screens/Console"),
  usage: () => import("./screens/Usage"),
};
type ScreenProps = { nav: Nav; id?: string };
function screen<K extends keyof typeof loaders>(key: K, name: string) {
  return lazy(() => loaders[key]().then((m) => ({ default: (m as unknown as Record<string, ComponentType<ScreenProps>>)[name] })));
}
const SettingsIndex = screen("settings", "SettingsIndex");
const GeneralSettings = screen("settings", "GeneralSettings");
const DnsSettings = screen("settings", "DnsSettings");
const ConnectionSettings = screen("settings", "ConnectionSettings");
const KillSwitchSettings = screen("settings", "KillSwitchSettings");
const SplitSettings = screen("settings", "SplitSettings");
const AdvancedSettings = screen("settings", "AdvancedSettings");
const LogsPage = screen("settings", "LogsPage");
const About = screen("settings", "About");
const Servers = screen("servers", "Servers");
const AddServer = screen("servers", "AddServer");
const ServerSetup = screen("servers", "ServerSetup");
const ServerEditor = screen("editor", "ServerEditor");
const Diagnostics = screen("tools", "Diagnostics");
const SpeedTest = screen("tools", "SpeedTest");
const Updates = screen("updates", "Updates");
const Console = screen("console", "Console");
const Usage = screen("usage", "Usage");

export type Route =
  | { name: "settings" | "general" | "dns" | "connection" | "split" | "killswitch" | "servers" | "add-server" | "setup" | "diagnostics" | "speed" | "advanced" | "about" | "logs" | "updates" | "console" | "usage" }
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
    case "split":
      return <SplitSettings nav={nav} />;
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
    case "updates":
      return <Updates nav={nav} />;
    case "console":
      return <Console nav={nav} />;
    case "usage":
      return <Usage nav={nav} />;
  }
}

function Backdrop({ on }: { on: boolean }) {
  return (
    <div className="backdrop" data-on={on} aria-hidden="true">
      <div className="orb orb-1" />
      <div className="orb orb-2" />
      <div className="orb orb-3" />
    </div>
  );
}

/** LOTUS / SHADOW switch: a capsule with a pill that slides on the compositor. */
function ModeSwitch({ mode, onChange }: { mode: Mode; onChange: (m: Mode) => void }) {
  const items: { value: Mode; label: string }[] = [
    { value: "lotus", label: "LOTUS" },
    { value: "shadow", label: "SHADOW" },
  ];
  return (
    <ToggleGroup
      type="single"
      spacing={0.5}
      value={mode}
      onValueChange={(v) => v && onChange(v as Mode)}
      aria-label="Mode"
      className="glass relative grid h-[36px] w-[200px] grid-cols-2 justify-self-center rounded-full p-[3px]"
    >
      <span
        aria-hidden="true"
        className={cn(
          "absolute top-[3px] bottom-[3px] left-[3px] w-[calc(50%-3px)] rounded-full transition-[translate,background-color] duration-200 ease-out",
          mode === "shadow" ? "translate-x-full bg-btn-a shadow-[inset_0_1px_0_rgb(255_255_255/0.16)]" : "bg-white shadow-[0_1px_3px_rgb(0_0_0/0.08),0_6px_16px_-8px_var(--shadow)]",
        )}
      />
      {items.map((it) => {
        const active = it.value === mode;
        return (
          <ToggleGroupItem
            key={it.value}
            value={it.value}
            className={cn(
              "relative h-full rounded-full pl-[0.22em] text-[11px] font-semibold tracking-[0.22em] transition-colors duration-150 hover:bg-transparent data-[state=on]:bg-transparent",
              active ? (it.value === "lotus" ? "text-primary" : "text-white") : "text-muted-foreground hover:text-foreground",
            )}
          >
            {it.label}
          </ToggleGroupItem>
        );
      })}
    </ToggleGroup>
  );
}

const pageVariants = {
  enter: (dir: number) => ({ x: dir > 0 ? 20 : -14, opacity: 0 }),
  center: { x: 0, opacity: 1 },
  exit: (dir: number) => ({ x: dir > 0 ? -14 : 20, opacity: 0 }),
};
// Critically damped: settles in about 200 ms without overshoot.
const pageTransition = { x: { type: "spring", stiffness: 720, damping: 54, mass: 0.7 }, opacity: { duration: 0.12, ease: easeOut } } as const;

export function App() {
  const ready = useStore((s) => s.ready);
  const settings = useStore((s) => s.settings);
  const status = useStore((s) => s.status);
  const service = useStore((s) => s.service);
  const updateReady = useStore((s) => s.update?.phase === "ready" || s.update?.phase === "available");
  const [stack, setStack] = useState<Route[]>([]);
  const [dir, setDir] = useState(1);
  const mode: Mode = settings?.mode ?? "lotus";

  const nav: Nav = {
    push: (r) => {
      setDir(1);
      setStack((s) => [...s, r]);
    },
    back: () => {
      setDir(-1);
      setStack((s) => s.slice(0, -1));
    },
    replace: (r) => {
      setDir(1);
      setStack((s) => [...s.slice(0, -1), r]);
    },
    home: () => {
      setDir(-1);
      setStack([]);
    },
  };

  useLayoutEffect(() => {
    const root = document.documentElement;
    root.dataset.mode = mode;
    root.dataset.motion = settings?.motion ?? "system";
  }, [mode, settings?.motion]);

  useEffect(() => {
    if (!ready) return;
    requestAnimationFrame(() => api.appReady());
    // Warm the other screens so the first visit to each opens instantly.
    const warm = () => Object.values(loaders).forEach((load) => void load());
    const idle = (window as Window & { requestIdleCallback?: (cb: () => void) => number }).requestIdleCallback;
    if (idle) idle(warm);
    else window.setTimeout(warm, 500);
  }, [ready]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented || document.querySelector("[role=dialog], [role=alertdialog], [role=listbox]")) return;
      setDir(-1);
      setStack((s) => s.slice(0, -1));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  async function switchMode(m: Mode) {
    if (!settings || m === settings.mode) return;
    const swap = () => setState({ settings: { ...settings, mode: m } });
    // One GPU crossfade of the whole window instead of animating every colour token.
    const doc = document as Document & { startViewTransition?: (cb: () => void) => unknown };
    const reduce = settings.motion === "reduce" || (settings.motion !== "full" && matchMedia("(prefers-reduced-motion: reduce)").matches);
    if (doc.startViewTransition && !reduce) doc.startViewTransition(() => flushSync(swap));
    else swap();
    try {
      setState({ settings: await api.setMode(m) });
    } catch (e) {
      toast(errorText(e), "error");
    }
  }

  if (!ready || !settings) return <Backdrop on={false} />;
  const on = visualFor(status, mode, service.connected, false) === "on";
  const top = stack[stack.length - 1];
  const reducedMotion = settings.motion === "reduce" ? "always" : settings.motion === "full" ? "never" : "user";

  return (
    <MotionConfig reducedMotion={reducedMotion}>
      <TooltipProvider>
        <Backdrop on={on && !top} />
        <div className="relative z-1 flex h-full flex-col">
          {/* The whole top strip moves the window; buttons inside it still click. */}
          <header className="grid h-[50px] shrink-0 grid-cols-[80px_1fr_80px] items-center px-2" onMouseDown={startWindowDrag}>
            <div className="relative flex">
              <IconButton label={updateReady ? "Settings (update available)" : "Settings"} onClick={() => (top?.name === "settings" ? nav.back() : (setDir(1), setStack([{ name: "settings" }])))}>
                <Settings className="size-[17px]" />
              </IconButton>
              <AnimatePresence>
                {updateReady && (
                  <motion.span
                    aria-hidden="true"
                    initial={{ scale: 0 }}
                    animate={{ scale: 1 }}
                    exit={{ scale: 0 }}
                    transition={spring}
                    className="pointer-events-none absolute top-[9px] left-[33px] size-2 rounded-full bg-primary ring-2 ring-[var(--bg-a)]"
                  />
                )}
              </AnimatePresence>
            </div>
            <ModeSwitch mode={mode} onChange={switchMode} />
            <div className="flex justify-end">
              <IconButton label="Minimize" onClick={minimizeWindow}>
                <Minus className="size-[17px]" />
              </IconButton>
              <IconButton label="Close" onClick={() => api.closeWindow()} className="hover:bg-destructive hover:text-white">
                <X className="size-[17px]" />
              </IconButton>
            </div>
          </header>
          <main className="relative min-h-0 flex-1">
            <motion.div
              className="absolute inset-0"
              initial={false}
              animate={top ? { opacity: 0, scale: 0.98 } : { opacity: 1, scale: 1 }}
              transition={{ duration: 0.16, ease: easeOut }}
              inert={!!top}
            >
              <Home nav={nav} />
            </motion.div>
            <AnimatePresence initial={false} custom={dir}>
              {top && (
                <motion.div key={stack.length + top.name} className="absolute inset-0" custom={dir} variants={pageVariants} initial="enter" animate="center" exit="exit" transition={pageTransition}>
                  <Suspense fallback={null}>
                    <Screen route={top} nav={nav} />
                  </Suspense>
                </motion.div>
              )}
            </AnimatePresence>
          </main>
        </div>
        <Toaster position="bottom-center" theme={mode === "shadow" ? "dark" : "light"} offset={16} gap={8} visibleToasts={3} />
      </TooltipProvider>
    </MotionConfig>
  );
}
