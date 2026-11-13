import { useEffect, useState } from "react";
import { Icon } from "../components/Icon";
import { PowerSwitch, type Visual } from "../components/PowerSwitch";
import { api, errorText } from "../lib/api";
import { country, duration, rate } from "../lib/format";
import { toast, useStore } from "../lib/store";
import type { Mode, Status } from "../lib/types";
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
    const t = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(t);
  }, [active]);
  return now;
}

const TRANSPORT_LABEL = { doh: "DNS-over-HTTPS", doh3: "DNS-over-HTTPS/3", dot: "DNS-over-TLS" } as const;

export function Home({ nav }: { nav: Nav }) {
  const settings = useStore((s) => s.settings)!;
  const status = useStore((s) => s.status);
  const service = useStore((s) => s.service);
  const profiles = useStore((s) => s.profiles);
  const providers = useStore((s) => s.providers);
  const storeWarning = useStore((s) => s.storeWarning);
  const [pending, setPending] = useState(false);
  const mode = settings.mode;
  const visual = visualFor(status, mode, service.connected, pending);
  const on = visual === "on";
  const now = useNow(on || visual === "reconnecting");
  const selected = profiles.find((p) => p.id === settings.selected_profile) ?? null;
  const provider = settings.lotus.provider === "custom" ? null : providers.find((p) => p.id === settings.lotus.provider);
  const providerName = settings.lotus.provider === "custom" ? "Custom resolver" : provider?.name ?? "";
  const live = status && status.mode === mode ? status : null;

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

  // ----- copy --------------------------------------------------------------
  const serverName = live?.profile_name ?? selected?.name ?? "your server";
  let label = "";
  let sub = "";
  if (mode === "lotus") {
    label = { off: "Not protected", connecting: "Securing DNS…", on: "DNS protected", reconnecting: "Reconnecting…", disconnecting: "Turning off…", error: "Needs attention" }[visual];
    sub = {
      off: "Encrypt every DNS lookup on this device. Your IP address stays the same.",
      connecting: "Starting the encrypted resolver",
      on: `Lookups are encrypted with ${providerName}. Your IP address stays the same.`,
      reconnecting: "Restarting encrypted DNS",
      disconnecting: "Restoring your original DNS settings",
      error: live?.error?.message ?? "Something went wrong",
    }[visual];
  } else {
    label = { off: "Not connected", connecting: "Connecting…", on: "Connected", reconnecting: "Reconnecting…", disconnecting: "Disconnecting…", error: "Connection failed" }[visual];
    sub = {
      off: selected ? `Route this whole device through ${selected.name}.` : "Add your own server to get started.",
      connecting: `Reaching ${serverName} securely`,
      on: `All traffic leaves the internet from ${serverName}.`,
      reconnecting: live?.kill_switch_engaged ? "The kill switch is holding traffic until the tunnel is back." : live?.error?.message ?? `Trying to reach ${serverName}`,
      disconnecting: "Restoring normal routing",
      error: live?.error?.message ?? "Something went wrong",
    }[visual];
  }

  const since = live?.connected_since ?? null;
  const elapsed = on && since ? duration(now - since) : "—";
  const blocking = [settings.lotus.block_trackers && "Trackers", (live?.protections.find((p) => p.id === "malware")?.active ?? (settings.lotus.block_malware && provider?.malware_variant)) && "Malware"]
    .filter(Boolean)
    .join(" · ");

  return (
    <div className="page home">
      <section className="hero">
        <div className="status-label" aria-live="polite">
          <span className="fade" key={`${mode}-${label}`}>
            {label}
          </span>
        </div>
        <PowerSwitch visual={visual} mode={mode} onToggle={toggle} disabled={!service.connected} />
        <div className={`status-sub ${visual === "error" ? "error" : ""}`}>
          <span className="fade" key={`${mode}-${sub}`}>
            {sub}
          </span>
        </div>
      </section>

      {!service.connected ? (
        <div className="banner">
          <Icon name="alert" />
          <div className="grow">
            <strong>{service.message ?? "Connecting to the LOTUS service…"}</strong>
            <div style={{ color: "var(--text-2)", marginTop: 2 }}>LOTUS needs its background service to change DNS and routing.</div>
            <div className="actions">
              <button
                className="btn small"
                onClick={async () => {
                  try {
                    await api.repairService();
                    toast("Service repaired");
                  } catch (e) {
                    toast(errorText(e), "error");
                  }
                }}
              >
                <Icon name="wrench" /> Repair service
              </button>
            </div>
          </div>
        </div>
      ) : mode === "lotus" ? (
        <div className="card info" key="lotus-card">
          <button className="row" onClick={() => nav.push({ name: "dns" })}>
            <span className="chip">
              <Icon name="lock" />
            </span>
            <span className="text">
              <span className="label">Encrypted resolver</span>
              <span className="value">
                {providerName} · {TRANSPORT_LABEL[settings.lotus.transport]}
              </span>
            </span>
            <span className="trail">
              <Icon name="chevron" />
            </span>
          </button>
          <button className="row" onClick={() => nav.push({ name: "dns" })}>
            <span className="chip">
              <Icon name="shield" />
            </span>
            <span className="text">
              <span className="label">Blocking</span>
              <span className="value">{blocking || "Off"}</span>
            </span>
            <span className="trail">
              <Icon name="chevron" />
            </span>
          </button>
          <div className="split">
            <div className="row">
              <span className="chip">
                <Icon name="globe" />
              </span>
              <span className="text">
                <span className="label">Your IP</span>
                <span className="value" title={live?.exit?.ipv4 ?? undefined}>
                  Unchanged
                </span>
              </span>
            </div>
            <div className="row">
              <span className="chip">
                <Icon name="clock" />
              </span>
              <span className="text">
                <span className="label">Protected for</span>
                <span className="value mono">{elapsed}</span>
              </span>
            </div>
          </div>
        </div>
      ) : (
        <div className="card info" key="shadow-card">
          <button className="row" onClick={() => nav.push({ name: selected ? "servers" : "add-server" })}>
            <span className="chip">
              <Icon name="server" />
            </span>
            <span className="text">
              <span className="label">Exit server</span>
              <span className="value">{selected?.name ?? "Add your server"}</span>
              {selected && <span className="sub">{live?.protocol_label ?? labelFor(selected.protocol.type)}</span>}
            </span>
            <span className="trail">
              <Icon name="chevron" />
            </span>
          </button>
          <div className="row">
            <span className="chip">
              <Icon name="globe" />
            </span>
            <span className="text">
              <span className="label">Exit IP</span>
              <span className="value mono">{on && live?.exit?.ipv4 ? live.exit.ipv4 : "—"}</span>
            </span>
            {on && live?.exit?.country && <span className="pill accent">{country(live.exit.country)}</span>}
          </div>
          <div className="split">
            <div className="row">
              <span className="chip">
                <Icon name="clock" />
              </span>
              <span className="text">
                <span className="label">Connected</span>
                <span className="value mono">{elapsed}</span>
              </span>
            </div>
            <div className="row">
              <span className="chip">
                <Icon name="pulse" />
              </span>
              <span className="text">
                <span className="label">Latency</span>
                <span className="value mono">{on && live?.latency_ms != null ? `${live.latency_ms} ms` : "—"}</span>
              </span>
            </div>
          </div>
        </div>
      )}

      {storeWarning && (
        <div className="banner" style={{ marginTop: 10 }}>
          <Icon name="alert" />
          <div className="grow">{storeWarning}</div>
        </div>
      )}

      <div className="footer">
        {mode === "shadow" && (on || visual === "reconnecting") ? (
          <div className="traffic" aria-label="Current traffic">
            <span className="up" title="Upload">
              <Icon name="up" /> {rate(live?.traffic.up_rate ?? 0)}
            </span>
            <span className="down" title="Download">
              <Icon name="down" /> {rate(live?.traffic.down_rate ?? 0)}
            </span>
          </div>
        ) : (
          <span className="pill" style={{ visibility: mode === "shadow" && settings.shadow.kill_switch ? "visible" : "hidden" }}>
            <Icon name="shield" /> Kill switch on
          </span>
        )}
        <button className="link-btn" onClick={() => nav.push({ name: "diagnostics" })}>
          <Icon name="shieldCheck" /> Privacy check
        </button>
      </div>
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
