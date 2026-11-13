import { useEffect, useState } from "react";
import { AppWindow, ArrowDownToLine, Copy, Cpu, Eye, FileText, Gauge, Info, Lock, Power, RefreshCw, Server, Shield, ShieldCheck, SlidersHorizontal, Wrench, Zap } from "lucide-react";
import { Advanced, Hint, IconButton, List, NavRow, Page, Pill, RadioList, Seg, SettingRow, ToggleRow } from "@/components/Controls";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { api, errorText } from "@/lib/api";
import { toast, updateSettings, useStore } from "@/lib/store";
import type { AppSettings, DnsProviderId, DnsTransport } from "@/lib/types";
import type { Nav } from "../App";
import { updateLabel } from "./Updates";

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
  return active ? <Hint>Changes apply the next time you connect.</Hint> : null;
}

export function SettingsIndex({ nav }: { nav: Nav }) {
  const settings = useStore((s) => s.settings)!;
  const profiles = useStore((s) => s.profiles);
  const providers = useStore((s) => s.providers);
  const update = useStore((s) => s.update);
  const pname = settings.lotus.provider === "custom" ? "Custom" : providers.find((p) => p.id === settings.lotus.provider)?.name;
  return (
    <Page title="Settings" onBack={nav.back}>
      <List>
        <NavRow icon={SlidersHorizontal} label="General" onClick={() => nav.push({ name: "general" })} />
        <NavRow icon={Lock} label="DNS & Privacy" value={pname} onClick={() => nav.push({ name: "dns" })} />
        <NavRow icon={Zap} label="Connection" onClick={() => nav.push({ name: "connection" })} />
        <NavRow icon={Shield} label="Kill switch" value={settings.shadow.kill_switch ? "On" : "Off"} onClick={() => nav.push({ name: "killswitch" })} />
        <NavRow icon={Server} label="Servers" value={profiles.length || undefined} onClick={() => nav.push({ name: "servers" })} />
      </List>
      <List>
        <NavRow icon={ShieldCheck} label="Privacy check" onClick={() => nav.push({ name: "diagnostics" })} />
        <NavRow icon={Gauge} label="Speed test" onClick={() => nav.push({ name: "speed" })} />
        <NavRow icon={ArrowDownToLine} label="Updates" value={updateLabel(update)} onClick={() => nav.push({ name: "updates" })} />
        <NavRow icon={Cpu} label="Advanced" onClick={() => nav.push({ name: "advanced" })} />
        <NavRow icon={Info} label="About" onClick={() => nav.push({ name: "about" })} />
      </List>
    </Page>
  );
}

export function GeneralSettings({ nav }: { nav: Nav }) {
  const s = useStore((st) => st.settings)!;
  return (
    <Page title="General" onBack={nav.back}>
      <List>
        <ToggleRow icon={Power} label="Launch at startup" checked={s.launch_at_startup} onChange={(v) => save((x) => ({ ...x, launch_at_startup: v }))} />
        <ToggleRow icon={Zap} label="Turn on when LOTUS starts" checked={s.connect_on_launch} onChange={(v) => save((x) => ({ ...x, connect_on_launch: v }))} />
        <ToggleRow
          icon={RefreshCw}
          label="Stay protected after a restart"
          sub="Reconnects before you sign in"
          checked={s.lotus.resume_on_boot && s.shadow.resume_on_boot}
          onChange={(v) => save((x) => ({ ...x, lotus: { ...x.lotus, resume_on_boot: v }, shadow: { ...x.shadow, resume_on_boot: v } }))}
        />
      </List>
      <List>
        <ToggleRow icon={AppWindow} label="Keep running in the tray" checked={s.close_to_tray} onChange={(v) => save((x) => ({ ...x, close_to_tray: v }))} />
        <ToggleRow icon={Power} label="Turn off when quitting" checked={s.disconnect_on_quit} onChange={(v) => save((x) => ({ ...x, disconnect_on_quit: v }))} />
      </List>
      <List>
        <SettingRow icon={Eye} label="Animations">
          <Seg
            label="Animations"
            value={s.motion}
            onChange={(v) => save((x) => ({ ...x, motion: v }))}
            options={[
              { value: "system", label: "System" },
              { value: "reduce", label: "Less" },
              { value: "full", label: "Full" },
            ]}
          />
        </SettingRow>
      </List>
    </Page>
  );
}

export function DnsSettings({ nav }: { nav: Nav }) {
  const s = useStore((st) => st.settings)!;
  const providers = useStore((st) => st.providers);
  const hint = useReconnectHint("lotus");
  const cur = providers.find((p) => p.id === s.lotus.provider);
  const [custom, setCustom] = useState(s.lotus.custom ?? { url: "", ip: "" });
  const setProvider = (provider: DnsProviderId) => {
    if (provider === "custom") {
      save((x) => ({ ...x, lotus: { ...x.lotus, provider: "custom", custom: x.lotus.custom ?? { url: "", ip: "" } } }));
      return;
    }
    save((x) => {
      const p = providers.find((q) => q.id === provider);
      let transport = x.lotus.transport;
      if (p && ((transport === "doh3" && !p.doh3) || (transport === "dot" && !p.dot))) transport = "doh";
      return { ...x, lotus: { ...x.lotus, provider, transport } };
    });
  };
  return (
    <Page title="DNS & Privacy" onBack={nav.back}>
      <List title="Resolver">
        <RadioList<DnsProviderId>
          label="Resolver"
          value={s.lotus.provider}
          onChange={setProvider}
          options={[...providers.map((p) => ({ value: p.id, label: p.name, sub: p.note })), { value: "custom" as const, label: "Custom", sub: "Your own DoH or DoT server" }]}
        />
      </List>
      {s.lotus.provider === "custom" && (
        <Card className="gap-3 p-4">
          <div className="grid gap-1.5">
            <Label htmlFor="cu" className="text-[12px] font-medium text-muted-foreground">
              Resolver URL
            </Label>
            <Input id="cu" placeholder="https://dns.example.net/dns-query" value={custom.url} onChange={(e) => setCustom({ ...custom, url: e.target.value })} />
          </div>
          <div className="grid gap-1.5">
            <Label htmlFor="ci" className="text-[12px] font-medium text-muted-foreground">
              Resolver IP address
            </Label>
            <Input id="ci" placeholder="192.0.2.53" value={custom.ip} onChange={(e) => setCustom({ ...custom, ip: e.target.value })} />
          </div>
          <Button size="sm" className="self-start" onClick={() => save((x) => ({ ...x, lotus: { ...x.lotus, custom } }))}>
            Save resolver
          </Button>
        </Card>
      )}
      {s.lotus.provider !== "custom" && (
        <List title="Protocol">
          <SettingRow label="Encryption">
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
        </List>
      )}
      <List title="Blocking">
        <ToggleRow icon={Eye} label="Trackers & ads" checked={s.lotus.block_trackers} onChange={(v) => save((x) => ({ ...x, lotus: { ...x.lotus, block_trackers: v } }))} />
        <ToggleRow
          icon={Shield}
          label="Malicious domains"
          sub={!cur?.malware_variant ? "Not offered by this resolver" : s.lotus.provider === "quad9" ? "Always on with Quad9" : undefined}
          checked={s.lotus.provider === "quad9" || (s.lotus.block_malware && !!cur?.malware_variant)}
          disabled={!cur?.malware_variant || s.lotus.provider === "quad9"}
          onChange={(v) => save((x) => ({ ...x, lotus: { ...x.lotus, block_malware: v } }))}
        />
      </List>
      <Advanced>
        <List>
          <ToggleRow
            icon={Lock}
            label="Block other DNS servers"
            sub="Stops apps with hard-coded resolvers"
            checked={s.lotus.block_plain_dns}
            onChange={(v) => save((x) => ({ ...x, lotus: { ...x.lotus, block_plain_dns: v } }))}
          />
        </List>
      </Advanced>
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
      <List>
        <ToggleRow icon={RefreshCw} label="Reconnect automatically" checked={s.shadow.auto_reconnect} onChange={(v) => save((x) => ({ ...x, shadow: { ...x.shadow, auto_reconnect: v } }))} />
        <ToggleRow icon={Eye} label="Block trackers & ads" checked={s.shadow.block_trackers} onChange={(v) => save((x) => ({ ...x, shadow: { ...x.shadow, block_trackers: v } }))} />
        <SettingRow icon={Server} label="IPv6" sub={s.shadow.ipv6 === "tunnel" ? "Blocked if your server lacks it" : undefined}>
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
      </List>
      <List title="DNS inside the tunnel" hint="Encrypted end to end, so neither your ISP nor your VPS host can read lookups.">
        <RadioList<DnsProviderId>
          label="Tunnel DNS"
          value={s.shadow.dns_provider}
          onChange={(v) => save((x) => ({ ...x, shadow: { ...x.shadow, dns_provider: v } }))}
          options={providers.map((p) => ({ value: p.id, label: p.name }))}
        />
      </List>
      <Advanced>
        <List>
          <SettingRow label="QUIC (HTTP/3)" sub={s.shadow.quic === "auto" ? "Blocked on TCP tunnels for speed" : undefined}>
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
          <SettingRow label="Tunnel MTU" sub="Empty uses the engine default">
            <Input
              className="h-8 w-[84px] text-center"
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
        </List>
      </Advanced>
      {hint}
    </Page>
  );
}

export function KillSwitchSettings({ nav }: { nav: Nav }) {
  const s = useStore((st) => st.settings)!;
  const hint = useReconnectHint("shadow");
  return (
    <Page title="Kill switch" onBack={nav.back}>
      <List hint="If SHADOW drops, Windows blocks everything outside the tunnel instead of leaking. The block lifts when you turn SHADOW off.">
        <ToggleRow icon={Shield} label="Kill switch" checked={s.shadow.kill_switch} onChange={(v) => save((x) => ({ ...x, shadow: { ...x.shadow, kill_switch: v } }))} />
        <ToggleRow icon={Server} label="Allow local network" sub="Printers, shares and your router" checked={s.shadow.allow_lan} onChange={(v) => save((x) => ({ ...x, shadow: { ...x.shadow, allow_lan: v } }))} />
      </List>
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
      <List>
        <SettingRow icon={Cpu} label="LOTUS service" sub={service.connected ? `v${status?.service_version ?? "?"}` : service.message ?? "Not reachable"}>
          <Pill tone={service.connected ? "good" : "bad"}>
            <span className="size-1.5 rounded-full bg-current" /> {service.connected ? "Running" : "Down"}
          </Pill>
        </SettingRow>
        <SettingRow icon={Zap} label="Networking engine" sub={`sing-box ${engine}`} />
        <NavRow icon={FileText} label="Engine log" onClick={() => nav.push({ name: "logs" })} />
      </List>
      <div className="flex flex-col items-start gap-2">
        <Button
          variant="secondary"
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
          <Wrench /> Repair service
        </Button>
        <Hint>Re-registers the background service. Needs administrator approval.</Hint>
      </div>
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
        <>
          <IconButton label="Copy" disabled={!lines?.length} onClick={() => api.copyText((lines ?? []).join("\n")).then(() => toast("Log copied"))}>
            <Copy />
          </IconButton>
          <IconButton label="Refresh" onClick={load}>
            <RefreshCw />
          </IconButton>
        </>
      }
    >
      <Card className="selectable max-h-[380px] overflow-auto p-3.5 font-mono text-[11px] leading-relaxed whitespace-pre-wrap text-muted-foreground select-text">
        {lines === null ? "Loading…" : lines.length ? lines.join("\n") : "Nothing logged. Normal operation leaves no log."}
      </Card>
    </Page>
  );
}

export function About({ nav }: { nav: Nav }) {
  const version = useStore((s) => s.version);
  const engine = useStore((s) => s.engineVersion);
  return (
    <Page title="About" onBack={nav.back}>
      <Card className="gap-1 p-5">
        <div className="text-[20px] font-semibold tracking-[0.28em]">LOTUS</div>
        <div className="text-muted-foreground">Version {version} · GPL-3.0-or-later</div>
        <div className="selectable text-[12px] text-faint select-text">github.com/HighSkai/Lotus</div>
      </Card>
      <List title="Privacy">
        <SettingRow label="No account, ads, analytics or telemetry" sub="Logs stay in memory with addresses removed. Profiles are encrypted with your Windows account." />
        <SettingRow label="Network checks" sub="Exit IP via Cloudflare's trace page. The privacy check also queries Google and Team Cymru. Speed tests use speed.cloudflare.com, only on demand." />
        <SettingRow label="Update checks" sub="Every few hours LOTUS asks GitHub for the latest release. Updates are signed and only install when you choose. Turn this off in Updates." />
      </List>
      <List title="Open-source components" hint="Full notices are in the licenses folder next to LOTUS.">
        <SettingRow label="sing-box" sub={`v${engine} · GPL-3.0-or-later`} />
        <SettingRow label="Tauri" sub="MIT / Apache-2.0" />
        <SettingRow label="React, Radix UI, shadcn/ui, Motion, Sonner, NumberFlow, Lucide" sub="MIT / ISC" />
        <SettingRow label="Inter" sub="SIL Open Font License 1.1" />
        <SettingRow label="Tracker list" sub="SagerNet/sing-geosite category-ads-all" />
      </List>
    </Page>
  );
}
