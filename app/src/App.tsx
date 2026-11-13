import { useEffect, useId, useLayoutEffect, useState } from "react";
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
import { About, AdvancedSettings, ConnectionSettings, DnsSettings, GeneralSettings, KillSwitchSettings, LogsPage, SettingsIndex } from "./screens/Settings";
import { AddServer, Servers, ServerSetup } from "./screens/Servers";
import { ServerEditor } from "./screens/ServerEditor";
import { Diagnostics, SpeedTest } from "./screens/Tools";
import { Updates } from "./screens/Updates";

export type Route =
  | { name: "settings" | "general" | "dns" | "connection" | "killswitch" | "servers" | "add-server" | "setup" | "diagnostics" | "speed" | "advanced" | "about" | "logs" | "updates" }
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
    case "updates":
      return <Updates nav={nav} />;
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

/** LOTUS / SHADOW switch, a glass capsule with a sliding pill. */
function ModeSwitch({ mode, onChange }: { mode: Mode; onChange: (m: Mode) => void }) {
  const id = useId();
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
      className="glass grid h-[36px] w-[200px] grid-cols-2 justify-self-center rounded-full p-[3px]"
    >
      {items.map((it) => {
        const active = it.value === mode;
        return (
          <ToggleGroupItem
            key={it.value}
            value={it.value}
            className={cn(
              "relative h-full rounded-full pl-[0.22em] text-[11px] font-semibold tracking-[0.22em] transition-colors duration-200 hover:bg-transparent data-[state=on]:bg-transparent",
              active ? (it.value === "lotus" ? "text-primary" : "text-white") : "text-faint hover:text-muted-foreground",
            )}
          >
            {active && (
              <motion.span
                layoutId={id}
                transition={spring}
                className={cn(
                  "absolute inset-0 rounded-full",
                  it.value === "lotus"
                    ? "bg-white shadow-[0_1px_3px_rgb(0_0_0/0.08),0_6px_16px_-8px_var(--shadow)]"
                    : "bg-linear-120 from-on-a to-on-b shadow-[inset_0_1px_0_rgb(255_255_255/0.18),0_6px_18px_-8px_var(--glow)]",
                )}
              />
            )}
            <span className="relative">{it.label}</span>
          </ToggleGroupItem>
        );
      })}
    </ToggleGroup>
  );
}

const pageVariants = {
  enter: (dir: number) => ({ x: dir > 0 ? 32 : -24, opacity: 0 }),
  center: { x: 0, opacity: 1 },
  exit: (dir: number) => ({ x: dir > 0 ? -24 : 32, opacity: 0 }),
};
const pageTransition = { x: { type: "spring", stiffness: 560, damping: 48, mass: 0.8 }, opacity: { duration: 0.16, ease: easeOut } } as const;

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
    if (ready) requestAnimationFrame(() => api.appReady());
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
              animate={top ? { opacity: 0, scale: 0.96 } : { opacity: 1, scale: 1 }}
              transition={{ duration: 0.22, ease: easeOut }}
              inert={!!top}
            >
              <Home nav={nav} />
            </motion.div>
            <AnimatePresence initial={false} custom={dir}>
              {top && (
                <motion.div key={stack.length + top.name} className="absolute inset-0" custom={dir} variants={pageVariants} initial="enter" animate="center" exit="exit" transition={pageTransition}>
                  <Screen route={top} nav={nav} />
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
