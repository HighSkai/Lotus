import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { ArrowDown, ArrowUp, Shield, ShieldCheck, TriangleAlert, Wrench } from "lucide-react";
import { NavRow, Notice, SettingRow } from "@/components/Controls";
import { PowerSwitch, type Visual } from "@/components/PowerSwitch";
import { SessionStats } from "@/components/SessionStats";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { api, errorText } from "@/lib/api";
import { country, rate } from "@/lib/format";
import { easeOut } from "@/lib/motion";
import { toast, useStore } from "@/lib/store";
import type { Mode, Status } from "@/lib/types";
import type { Nav } from "../App";

export function visualFor(status: Status | null, mode: Mode, serviceUp: boolean, pending: boolean): Visual {
  if (!serviceUp || !status) return "off";
  if (status.mode && status.mode !== mode) {
    return status.state === "connecting" || pending ? "connecting" : "off";
  }
  switch (status.state) {
    case "connected":
      return "on";
    case "connecting":
      return "connecting";
    case "reconnecting":
      return "reconnecting";
    case "disconnecting":
      return "disconnecting";
    case "error":
      return pending ? "connecting" : "error";
    default:
      return pending ? "connecting" : "off";
  }
}

function useNow(active: boolean) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const t = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(t);
  }, [active]);
  return now;
}

const TRANSPORT = { doh: "HTTPS", doh3: "HTTP/3", dot: "TLS" } as const;

const LABELS: Record<Mode, Record<Visual, string>> = {
  lotus: { off: "Not protected", connecting: "Connecting", on: "Protected", reconnecting: "Reconnecting", disconnecting: "Turning off", error: "Needs attention" },
  shadow: { off: "Not connected", connecting: "Connecting", on: "Connected", reconnecting: "Reconnecting", disconnecting: "Disconnecting", error: "Connection failed" },
};

export function Home({ nav }: { nav: Nav }) {
  const settings = useStore((s) => s.settings)!;
  const status = useStore((s) => s.status);
  const service = useStore((s) => s.service);
  const profiles = useStore((s) => s.profiles);
  const providers = useStore((s) => s.providers);
  const storeWarning = useStore((s) => s.storeWarning);
  const last = useStore((s) => s.lastSession);
  const [pending, setPending] = useState(false);
  const mode = settings.mode;
  const visual = visualFor(status, mode, service.connected, pending);
  const on = visual === "on";
  const live = status && status.mode === mode ? status : null;
  const running = on || visual === "reconnecting";
  const now = useNow(running);
  const selected = profiles.find((p) => p.id === settings.selected_profile) ?? null;
  const provider = settings.lotus.provider === "custom" ? null : providers.find((p) => p.id === settings.lotus.provider);
  const providerName = settings.lotus.provider === "custom" ? "Custom resolver" : provider?.name ?? "";
  const reduced = settings.motion === "reduce";

  async function toggle() {
    if (visual === "on" || visual === "connecting" || visual === "reconnecting") {
      setPending(false);
      try {
        await api.disconnect();
      } catch (e) {
        toast(errorText(e), "error");
      }
      return;
    }
    if (mode === "shadow" && !selected) {
      nav.push({ name: "add-server" });
      return;
    }
    setPending(true);
    try {
      await api.connect();
    } catch (e) {
      toast(errorText(e), "error");
    } finally {
      window.setTimeout(() => setPending(false), 400);
    }
  }

  const label = LABELS[mode][visual];
  const idle = mode === "lotus" ? "Encrypt DNS without changing your IP." : selected ? `Exit through ${selected.name}` : "Add a server to get started.";
  const blocking = [settings.lotus.block_trackers && "Trackers", (live?.protections.find((p) => p.id === "malware")?.active ?? (settings.lotus.block_malware && provider?.malware_variant)) && "Malware"]
    .filter(Boolean)
    .join(" · ");

  return (
    <div className="flex h-full flex-col px-4 pb-3">
      <section className="flex min-h-[236px] flex-1 flex-col items-center justify-center gap-4">
        <div className="grid h-8 place-items-center" aria-live="polite">
          <AnimatePresence initial={false}>
            <motion.h2
              key={`${mode}-${label}`}
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -6 }}
              transition={{ duration: 0.14, ease: easeOut }}
              className="m-0 text-[24px] font-semibold tracking-[-0.025em] [grid-area:1/1]"
            >
              {label}
            </motion.h2>
          </AnimatePresence>
        </div>
        <PowerSwitch visual={visual} mode={mode} onToggle={toggle} disabled={!service.connected} />
        <SessionStats visual={visual} mode={mode} live={live} now={now} last={last} idle={idle} animated={!reduced} />
      </section>

      <AnimatePresence mode="wait" initial={false}>
        <motion.div
          key={!service.connected ? "down" : mode}
          initial={{ opacity: 0, y: 6 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: -3 }}
          transition={{ duration: 0.14, ease: easeOut }}
        >
          {!service.connected ? (
            <Notice icon={TriangleAlert}>
              <div className="font-semibold">{service.message ?? "Connecting to the LOTUS service…"}</div>
              <Button
                size="sm"
                variant="secondary"
                className="mt-2.5"
                onClick={async () => {
                  try {
                    await api.repairService();
                    toast("Service repaired");
                  } catch (e) {
                    toast(errorText(e), "error");
                  }
                }}
              >
                <Wrench /> Repair service
              </Button>
            </Notice>
          ) : mode === "lotus" ? (
            <Card className="p-1">
              <NavRow label="Resolver" value={`${providerName} · ${TRANSPORT[settings.lotus.transport]}`} onClick={() => nav.push({ name: "dns" })} />
              <NavRow label="Blocking" value={blocking || "Off"} onClick={() => nav.push({ name: "dns" })} />
            </Card>
          ) : (
            <Card className="p-1">
              <NavRow label="Server" value={selected?.name ?? "Add a server"} onClick={() => nav.push({ name: selected ? "servers" : "add-server" })} />
              <SettingRow label="Exit IP">
                <span className="truncate">
                  {running && live?.exit?.ipv4 ? (
                    <>
                      <span className="font-mono text-[12.5px] text-foreground tabular-nums">{live.exit.ipv4}</span>
                      {live.exit.country && ` · ${country(live.exit.country)}`}
                    </>
                  ) : (
                    "—"
                  )}
                </span>
              </SettingRow>
              {selected && <NavRow label="Live console" onClick={() => nav.push({ name: "console" })} />}
            </Card>
          )}
        </motion.div>
      </AnimatePresence>

      {storeWarning && (
        <div className="mt-2.5">
          <Notice icon={TriangleAlert}>{storeWarning}</Notice>
        </div>
      )}

      <footer className="flex h-11 shrink-0 items-center justify-between gap-2 pt-2.5">
        {mode === "shadow" && running ? (
          live?.kill_switch_engaged && visual === "reconnecting" ? (
            <span className="inline-flex items-center gap-1.5 pl-1 text-[12px] text-warn">
              <Shield className="size-3.5" /> Kill switch holding traffic
            </span>
          ) : (
            <div className="flex gap-3.5 pl-1 font-mono text-[11.5px] text-muted-foreground tabular-nums" aria-label="Current traffic">
              <span className="inline-flex min-w-[78px] items-center gap-1">
                <ArrowUp className="size-3 text-accent-2" /> {rate(live?.traffic.up_rate ?? 0)}
              </span>
              <span className="inline-flex min-w-[78px] items-center gap-1">
                <ArrowDown className="size-3 text-primary" /> {rate(live?.traffic.down_rate ?? 0)}
              </span>
            </div>
          )
        ) : mode === "shadow" && settings.shadow.kill_switch ? (
          <span className="inline-flex items-center gap-1.5 pl-1 text-[12px] text-muted-foreground">
            <Shield className="size-3.5" /> Kill switch on
          </span>
        ) : (
          <span />
        )}
        <Button variant="ghost" size="sm" onClick={() => nav.push({ name: "diagnostics" })}>
          <ShieldCheck /> Privacy check
        </Button>
      </footer>
    </div>
  );
}

export function labelFor(t: string): string {
  return (
    {
      vless: "VLESS",
      trojan: "Trojan",
      shadowsocks: "Shadowsocks",
      hysteria2: "Hysteria2",
      tuic: "TUIC",
      anytls: "AnyTLS",
      naive: "NaiveProxy",
      wireguard: "WireGuard",
    } as Record<string, string>
  )[t] ?? t;
}
