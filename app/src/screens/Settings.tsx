import { useEffect, useState } from "react";
import { Icon } from "../components/Icon";
import { NavRow, Page, RadioRow, Seg, SettingRow, ToggleRow } from "../components/Controls";
import { api, errorText } from "../lib/api";
import { toast, updateSettings, useStore } from "../lib/store";
import type { AppSettings, DnsProviderId, DnsTransport } from "../lib/types";
import type { Nav } from "../App";

async function save(fn: (s: AppSettings) => AppSettings) {
  try {
    await updateSettings(fn);
  } catch (e) {
    toast(errorText(e), "error");
  }
}

/** Changes that only take effect on the next connection get a gentle hint. */
function useReconnectHint(scope: "lotus" | "shadow") {
  const status = useStore((s) => s.status);
  const active = status?.mode === scope && (status.state === "connected" || status.state === "reconnecting");
  return active ? <p className="hint">Changes apply the next time you connect.</p> : null;
}

export function SettingsIndex({ nav }: { nav: Nav }) {
  const settings = useStore((s) => s.settings)!;
  const profiles = useStore((s) => s.profiles);
  const providers = useStore((s) => s.providers);
  const pname = settings.lotus.provider === "custom" ? "Custom" : providers.find((p) => p.id === settings.lotus.provider)?.name;
  return (
    <Page title="Settings" onBack={nav.back}>
      <div className="card list">
        <NavRow icon="sliders" label="General" sub="Startup, window and tray" onClick={() => nav.push({ name: "general" })} />
        <NavRow icon="lock" label="DNS & Privacy" sub={`LOTUS · ${pname}`} onClick={() => nav.push({ name: "dns" })} />
        <NavRow icon="zap" label="Connection" sub="SHADOW tunnel behaviour" onClick={() => nav.push({ name: "connection" })} />
        <NavRow icon="shield" label="Kill switch" sub={settings.shadow.kill_switch ? "On" : "Off"} onClick={() => nav.push({ name: "killswitch" })} />
        <NavRow icon="server" label="Server profiles" sub={`${profiles.length} saved`} onClick={() => nav.push({ name: "servers" })} />
      </div>
      <div className="card list">
        <NavRow icon="shieldCheck" label="Privacy check" sub="DNS, IPv4 and IPv6 leak tests" onClick={() => nav.push({ name: "diagnostics" })} />
        <NavRow icon="speed" label="Speed test" sub="Measure on demand" onClick={() => nav.push({ name: "speed" })} />
        <NavRow icon="cpu" label="Advanced" sub="Service, engine and logs" onClick={() => nav.push({ name: "advanced" })} />
        <NavRow icon="info" label="About LOTUS" onClick={() => nav.push({ name: "about" })} />
      </div>
    </Page>
  );
}

export function GeneralSettings({ nav }: { nav: Nav }) {
  const s = useStore((st) => st.settings)!;
  return (
    <Page title="General" onBack={nav.back}>
      <div className="card list">
        <ToggleRow icon="power" label="Launch at startup" sub="Start quietly in the tray when you sign in" checked={s.launch_at_startup} onChange={(v) => save((x) => ({ ...x, launch_at_startup: v }))} />
        <ToggleRow icon="zap" label="Turn on when LOTUS starts" sub="Connect the selected mode automatically" checked={s.connect_on_launch} onChange={(v) => save((x) => ({ ...x, connect_on_launch: v }))} />
        <ToggleRow
          icon="refresh"
          label="Stay protected after a restart"
          sub="The service reconnects before you sign in"
          checked={s.lotus.resume_on_boot && s.shadow.resume_on_boot}
          onChange={(v) => save((x) => ({ ...x, lotus: { ...x.lotus, resume_on_boot: v }, shadow: { ...x.shadow, resume_on_boot: v } }))}
        />
      </div>
      <div className="card list">
        <ToggleRow icon="list" label="Keep running in the tray" sub="Closing the window doesn't quit LOTUS" checked={s.close_to_tray} onChange={(v) => save((x) => ({ ...x, close_to_tray: v }))} />
        <ToggleRow icon="power" label="Turn off when quitting" sub="Otherwise protection keeps running without the window" checked={s.disconnect_on_quit} onChange={(v) => save((x) => ({ ...x, disconnect_on_quit: v }))} />
      </div>
      <div className="card list">
        <SettingRow icon="eye" label="Animations">
          <Seg
            label="Animations"
            value={s.motion}
            onChange={(v) => save((x) => ({ ...x, motion: v }))}
            options={[
              { value: "system", label: "System" },
              { value: "reduce", label: "Reduced" },
              { value: "full", label: "Full" },
            ]}
          />
        </SettingRow>
      </div>
    </Page>
  );
}

export function DnsSettings({ nav }: { nav: Nav }) {
  const s = useStore((st) => st.settings)!;
  const providers = useStore((st) => st.providers);
  const hint = useReconnectHint("lotus");
  const cur = providers.find((p) => p.id === s.lotus.provider);
  const [custom, setCustom] = useState(s.lotus.custom ?? { url: "", ip: "" });
  const setProvider = (provider: DnsProviderId) =>
    save((x) => {
      const p = providers.find((q) => q.id === provider);
      let transport = x.lotus.transport;
      if (p && ((transport === "doh3" && !p.doh3) || (transport === "dot" && !p.dot))) transport = "doh";
      return { ...x, lotus: { ...x.lotus, provider, transport } };
    });
  return (
    <Page title="DNS & Privacy" onBack={nav.back}>
      <p className="hint" style={{ marginTop: 0 }}>
        LOTUS mode sends every DNS lookup through an encrypted resolver instead of your internet provider. It doesn't hide your IP address. For that, use SHADOW.
      </p>
      <div className="section-title">Resolver</div>
      <div className="card list" role="radiogroup" aria-label="Resolver">
        {providers.map((p) => (
          <RadioRow key={p.id} label={p.name} sub={p.note} checked={s.lotus.provider === p.id} onSelect={() => setProvider(p.id)} />
        ))}
        <RadioRow label="Custom" sub="Your own DoH or DoT server" checked={s.lotus.provider === "custom"} onSelect={() => save((x) => ({ ...x, lotus: { ...x.lotus, provider: "custom", custom: x.lotus.custom ?? { url: "", ip: "" } } }))} />
      </div>
      {s.lotus.provider === "custom" && (
        <div className="card" style={{ padding: 14, marginTop: 10 }}>
          <div className="field">
            <label htmlFor="cu">Resolver URL</label>
            <input id="cu" className="input" placeholder="https://dns.example.net/dns-query or tls://dns.example.net" value={custom.url} onChange={(e) => setCustom({ ...custom, url: e.target.value })} />
          </div>
          <div className="field">
            <label htmlFor="ci">Resolver IP address</label>
            <input id="ci" className="input" placeholder="192.0.2.53" value={custom.ip} onChange={(e) => setCustom({ ...custom, ip: e.target.value })} />
          </div>
          <button className="btn small primary" onClick={() => save((x) => ({ ...x, lotus: { ...x.lotus, custom } }))}>
            Save resolver
          </button>
          <p className="hint">The IP is used to reach the resolver, so no unencrypted lookup is needed first. The certificate is still verified against the host name.</p>
        </div>
      )}
      {s.lotus.provider !== "custom" && (
        <>
          <div className="section-title">Protocol</div>
          <div className="card list">
            <SettingRow label="Encryption" sub={s.lotus.transport === "doh3" ? "Fastest over QUIC" : s.lotus.transport === "dot" ? "Dedicated port 853" : "Looks like normal web traffic"}>
              <Seg<DnsTransport>
                label="DNS protocol"
                value={s.lotus.transport}
                onChange={(v) => save((x) => ({ ...x, lotus: { ...x.lotus, transport: v } }))}
                options={[
                  { value: "doh", label: "HTTPS" },
                  { value: "doh3", label: "HTTP/3", disabled: !cur?.doh3 },
                  { value: "dot", label: "TLS", disabled: !cur?.dot },
                ]}
              />
            </SettingRow>
          </div>
        </>
      )}
      <div className="section-title">Blocking</div>
      <div className="card list">
        <ToggleRow icon="eye" label="Block trackers & ads" sub="On-device list of known tracking domains" checked={s.lotus.block_trackers} onChange={(v) => save((x) => ({ ...x, lotus: { ...x.lotus, block_trackers: v } }))} />
        <ToggleRow
          icon="shield"
          label="Block malicious domains"
          sub={cur?.malware_variant ? (s.lotus.provider === "quad9" ? "Always on with Quad9" : `Uses ${cur.name}'s filtering resolver`) : "Not offered by this resolver"}
          checked={s.lotus.provider === "quad9" || (s.lotus.block_malware && !!cur?.malware_variant)}
          disabled={!cur?.malware_variant || s.lotus.provider === "quad9"}
          onChange={(v) => save((x) => ({ ...x, lotus: { ...x.lotus, block_malware: v } }))}
        />
      </div>
      <details className="advanced">
        <summary>
          <Icon name="chevron" /> Advanced
        </summary>
        <div className="card list">
          <ToggleRow
            icon="lock"
            label="Block other DNS servers"
            sub="Stops apps with hard-coded resolvers from sending unencrypted DNS"
            checked={s.lotus.block_plain_dns}
            onChange={(v) => save((x) => ({ ...x, lotus: { ...x.lotus, block_plain_dns: v } }))}
          />
        </div>
      </details>
      {hint}
    </Page>
  );
}

export function ConnectionSettings({ nav }: { nav: Nav }) {
  const s = useStore((st) => st.settings)!;
  const providers = useStore((st) => st.providers);
  const hint = useReconnectHint("shadow");
  const [mtu, setMtu] = useState(s.shadow.mtu ? String(s.shadow.mtu) : "");
  return (
    <Page title="Connection" onBack={nav.back}>
      <div className="card list">
        <ToggleRow icon="refresh" label="Reconnect automatically" sub="After network changes, sleep or a server hiccup" checked={s.shadow.auto_reconnect} onChange={(v) => save((x) => ({ ...x, shadow: { ...x.shadow, auto_reconnect: v } }))} />
        <ToggleRow icon="eye" label="Block trackers & ads" sub="Also inside the tunnel" checked={s.shadow.block_trackers} onChange={(v) => save((x) => ({ ...x, shadow: { ...x.shadow, block_trackers: v } }))} />
        <SettingRow icon="globe" label="IPv6" sub={s.shadow.ipv6 === "tunnel" ? "Tunnelled when your server has IPv6, otherwise blocked" : "Always blocked"}>
          <Seg
            label="IPv6"
            value={s.shadow.ipv6}
            onChange={(v) => save((x) => ({ ...x, shadow: { ...x.shadow, ipv6: v } }))}
            options={[
              { value: "tunnel", label: "Tunnel" },
              { value: "disable", label: "Off" },
            ]}
          />
        </SettingRow>
      </div>
      <div className="section-title">DNS inside the tunnel</div>
      <div className="card list" role="radiogroup" aria-label="Tunnel DNS">
        {providers.map((p) => (
          <RadioRow key={p.id} label={p.name} checked={s.shadow.dns_provider === p.id} onSelect={() => save((x) => ({ ...x, shadow: { ...x.shadow, dns_provider: p.id } }))} />
        ))}
      </div>
      <p className="hint">Queries travel encrypted (DNS-over-HTTPS) through your server, so neither your ISP nor your VPS host can read them.</p>
      <details className="advanced">
        <summary>
          <Icon name="chevron" /> Advanced
        </summary>
        <div className="card list">
          <SettingRow label="QUIC (HTTP/3)" sub={s.shadow.quic === "auto" ? "Blocked on TCP tunnels so browsers use HTTP/2, which is faster through TCP" : s.shadow.quic === "block" ? "Always blocked" : "Always allowed"}>
            <Seg
              label="QUIC"
              value={s.shadow.quic}
              onChange={(v) => save((x) => ({ ...x, shadow: { ...x.shadow, quic: v } }))}
              options={[
                { value: "auto", label: "Auto" },
                { value: "block", label: "Block" },
                { value: "allow", label: "Allow" },
              ]}
            />
          </SettingRow>
          <SettingRow label="Tunnel MTU" sub="Leave empty for the engine default (best for most networks)">
            <input
              className="input"
              style={{ width: 86, minHeight: 32, textAlign: "center" }}
              inputMode="numeric"
              placeholder="auto"
              value={mtu}
              onChange={(e) => setMtu(e.target.value.replace(/\D/g, "").slice(0, 4))}
              onBlur={() => {
                const n = mtu ? Number(mtu) : undefined;
                save((x) => ({ ...x, shadow: { ...x.shadow, mtu: n } }));
              }}
            />
          </SettingRow>
        </div>
      </details>
      {hint}
    </Page>
  );
}

export function KillSwitchSettings({ nav }: { nav: Nav }) {
  const s = useStore((st) => st.settings)!;
  const hint = useReconnectHint("shadow");
  return (
    <Page title="Kill switch" onBack={nav.back}>
      <div className="card list">
        <ToggleRow icon="shield" label="Kill switch" sub="If the tunnel drops, block internet access instead of leaking" checked={s.shadow.kill_switch} onChange={(v) => save((x) => ({ ...x, shadow: { ...x.shadow, kill_switch: v } }))} />
        <ToggleRow icon="list" label="Allow local network" sub="Printers, file shares and your router stay reachable" checked={s.shadow.allow_lan} onChange={(v) => save((x) => ({ ...x, shadow: { ...x.shadow, allow_lan: v } }))} />
      </div>
      <p className="hint">
        While SHADOW is on, Windows Filtering Platform rules allow only the tunnel. They stay in place if the connection or even the LOTUS service fails, and are removed as soon as you turn SHADOW off. With automatic reconnect off, traffic stays blocked after a drop until you reconnect or turn SHADOW off.
      </p>
      {hint}
    </Page>
  );
}

export function AdvancedSettings({ nav }: { nav: Nav }) {
  const service = useStore((s) => s.service);
  const status = useStore((s) => s.status);
  const engine = useStore((s) => s.engineVersion);
  const [busy, setBusy] = useState(false);
  return (
    <Page title="Advanced" onBack={nav.back}>
      <div className="card list">
        <SettingRow icon="cpu" label="LOTUS service" sub={service.connected ? `Running · v${status?.service_version ?? "?"}` : service.message ?? "Not reachable"}>
          <span className={`pill ${service.connected ? "good" : "bad"}`}>
            <span className="dot" /> {service.connected ? "OK" : "Down"}
          </span>
        </SettingRow>
        <SettingRow icon="zap" label="Networking engine" sub={`sing-box ${engine} · integrity-checked before every start`} />
        <NavRow icon="file" label="Engine log" sub="Warnings and errors only, addresses redacted" onClick={() => nav.push({ name: "logs" })} />
      </div>
      <div className="btn-row" style={{ marginTop: 12 }}>
        <button
          className="btn"
          disabled={busy}
          onClick={async () => {
            setBusy(true);
            try {
              await api.repairService();
              toast("Service reinstalled and started");
            } catch (e) {
              toast(errorText(e), "error");
            } finally {
              setBusy(false);
            }
          }}
        >
          <Icon name="wrench" /> Repair service
        </button>
      </div>
      <p className="hint">Repair re-registers the background service. Windows will ask for administrator approval.</p>
    </Page>
  );
}

export function LogsPage({ nav }: { nav: Nav }) {
  const [lines, setLines] = useState<string[] | null>(null);
  const load = () =>
    api
      .logs()
      .then(setLines)
      .catch((e) => setLines([errorText(e)]));
  useEffect(() => {
    load();
  }, []);
  return (
    <Page
      title="Engine log"
      onBack={nav.back}
      actions={
        <button className="icon-btn" onClick={load} aria-label="Refresh">
          <Icon name="refresh" />
        </button>
      }
    >
      <div className="card logbox">{lines === null ? "Loading…" : lines.length ? lines.join("\n") : "No messages. The engine runs at warning level, so normal operation leaves no log."}</div>
      <button className="btn small" style={{ marginTop: 10 }} disabled={!lines?.length} onClick={() => api.copyText((lines ?? []).join("\n")).then(() => toast("Log copied"))}>
        <Icon name="copy" /> Copy
      </button>
    </Page>
  );
}

export function About({ nav }: { nav: Nav }) {
  const version = useStore((s) => s.version);
  const engine = useStore((s) => s.engineVersion);
  return (
    <Page title="About" onBack={nav.back}>
      <div className="card" style={{ padding: 18 }}>
        <div style={{ fontFamily: "var(--font-display)", fontSize: 20, fontWeight: 650, letterSpacing: "0.28em" }}>LOTUS</div>
        <div style={{ color: "var(--text-2)", marginTop: 2 }}>Version {version} · Free software under GPL-3.0-or-later</div>
        <div style={{ color: "var(--text-3)", marginTop: 2, fontSize: 12, userSelect: "text" }}>github.com/HighSkai/Lotus</div>
        <p className="hint" style={{ marginLeft: 0 }}>
          No account, no ads, no analytics, no telemetry. LOTUS never collects browsing history. Engine logs stay in memory at warning level with addresses removed. Server profiles are encrypted with your Windows account (DPAPI).
        </p>
        <p className="hint" style={{ marginLeft: 0 }}>
          To verify connections, LOTUS contacts Cloudflare (1.1.1.1/cdn-cgi/trace) to read your exit IP. The privacy check also queries Google's diagnostic DNS name and Team Cymru's IP-to-network lookup. A speed test uses speed.cloudflare.com and only runs when you start one.
        </p>
      </div>
      <div className="section-title">Open-source components</div>
      <div className="card list">
        <SettingRow label="sing-box" sub={`v${engine} · GPL-3.0-or-later · github.com/SagerNet/sing-box`} />
        <SettingRow label="Tauri" sub="MIT / Apache-2.0" />
        <SettingRow label="React" sub="MIT" />
        <SettingRow label="Tracker list" sub="SagerNet/sing-geosite category-ads-all" />
      </div>
      <p className="hint">Full notices are installed in the licenses folder next to LOTUS.</p>
    </Page>
  );
}
