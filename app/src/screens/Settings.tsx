import { useEffect, useState } from "react";
import { ArrowDownToLine, ChartColumn, Copy, Cpu, Gauge, Info, Lock, Plus, RefreshCw, Server, Shield, ShieldCheck, SlidersHorizontal, Split, Wrench, X, Zap } from "lucide-react";
import { Advanced, Hint, IconButton, List, NavRow, Notice, Page, Pill, RadioList, Seg, SectionTitle, SettingRow, ToggleRow } from "@/components/Controls";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { api, errorText } from "@/lib/api";
import { normalizeApp, normalizeSite, splitLabel } from "@/lib/format";
import { toast, updateSettings, useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import type { AppSettings, DnsProviderId, DnsTransport, ShadowOptions, SplitMode, SplitTunnel } from "@/lib/types";
import type { Nav } from "../App";
import { updateLabel } from "./Updates";

async function save(fn: (s: AppSettings) => AppSettings) {
  try {
    await updateSettings(fn);
  } catch (e) {
    toast(errorText(e), "error");
  }
}

const MOTION_HINT: Record<AppSettings["motion"], string> = {
  system: "Follows Windows: Full, or Less when animation effects are off.",
  full: "Springs, slides and small flourishes.",
  reduce: "Short, smooth glides and fades. Nothing bounces, spins or loops.",
  none: "Every change is instant.",
};

const ENCRYPT = {
  all: { label: "Traffic and DNS", sub: () => "Everything goes through your server" },
  traffic: { label: "Traffic only", sub: (r: string) => `Lookups go straight to ${r}, still encrypted. Faster, but ${r} sees your IP` },
  dns: { label: "DNS only", sub: () => "Only lookups go through your server. Websites see your IP" },
} as const;

/** Changes apply to a live session at once; say so while it reconnects. */
function useReconnectHint(scope: "lotus" | "shadow") {
  const status = useStore((s) => s.status);
  const applying = status?.mode === scope && (status.state === "connecting" || status.state === "disconnecting");
  return applying ? <Hint>Applying your change…</Hint> : null;
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
        <NavRow icon={Zap} label="Connection" value={settings.shadow.encrypt === "all" ? undefined : ENCRYPT[settings.shadow.encrypt].label} onClick={() => nav.push({ name: "connection" })} />
        <NavRow icon={Split} label="Split tunnelling" value={splitLabel(settings.shadow.split)} onClick={() => nav.push({ name: "split" })} />
        <NavRow icon={Shield} label="Kill switch" value={settings.shadow.kill_switch ? "On" : "Off"} onClick={() => nav.push({ name: "killswitch" })} />
        <NavRow icon={Server} label="Servers" value={profiles.length || undefined} onClick={() => nav.push({ name: "servers" })} />
        <NavRow icon={ChartColumn} label="Usage" onClick={() => nav.push({ name: "usage" })} />
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
        <ToggleRow label="Launch at startup" checked={s.launch_at_startup} onChange={(v) => save((x) => ({ ...x, launch_at_startup: v }))} />
        <ToggleRow label="Turn on when LOTUS starts" checked={s.connect_on_launch} onChange={(v) => save((x) => ({ ...x, connect_on_launch: v }))} />
        <ToggleRow
          label="Stay protected after a restart"
          sub="Reconnects before you sign in"
          checked={s.lotus.resume_on_boot && s.shadow.resume_on_boot}
          onChange={(v) => save((x) => ({ ...x, lotus: { ...x.lotus, resume_on_boot: v }, shadow: { ...x.shadow, resume_on_boot: v } }))}
        />
      </List>
      <List>
        <ToggleRow label="Keep running in the tray" checked={s.close_to_tray} onChange={(v) => save((x) => ({ ...x, close_to_tray: v }))} />
        <ToggleRow label="Turn off when quitting" checked={s.disconnect_on_quit} onChange={(v) => save((x) => ({ ...x, disconnect_on_quit: v }))} />
      </List>
      <List title="Animations" hint={MOTION_HINT[s.motion]}>
        <div className="p-1.5">
          <Seg
            label="Animations"
            className="w-full"
            value={s.motion}
            onChange={(v) => save((x) => ({ ...x, motion: v }))}
            options={[
              { value: "system", label: "System" },
              { value: "full", label: "Full" },
              { value: "reduce", label: "Less" },
              { value: "none", label: "None" },
            ]}
          />
        </div>
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
        <ToggleRow label="Trackers & ads" checked={s.lotus.block_trackers} onChange={(v) => save((x) => ({ ...x, lotus: { ...x.lotus, block_trackers: v } }))} />
        <ToggleRow
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
  const resolver = providers.find((p) => p.id === s.shadow.dns_provider)?.name ?? "your resolver";
  return (
    <Page title="Connection" onBack={nav.back}>
      <List title="Send through your server">
        <RadioList<ShadowOptions["encrypt"]>
          label="Send through your server"
          value={s.shadow.encrypt}
          onChange={(v) => save((x) => ({ ...x, shadow: { ...x.shadow, encrypt: v } }))}
          options={(["all", "traffic", "dns"] as const).map((v) => ({ value: v, label: ENCRYPT[v].label, sub: ENCRYPT[v].sub(resolver) }))}
        />
      </List>
      <List>
        <NavRow label="Split tunnelling" value={splitLabel(s.shadow.split)} onClick={() => nav.push({ name: "split" })} />
      </List>
      <List>
        <ToggleRow label="Reconnect automatically" checked={s.shadow.auto_reconnect} onChange={(v) => save((x) => ({ ...x, shadow: { ...x.shadow, auto_reconnect: v } }))} />
        <ToggleRow label="Block trackers & ads" checked={s.shadow.block_trackers} onChange={(v) => save((x) => ({ ...x, shadow: { ...x.shadow, block_trackers: v } }))} />
        <SettingRow label="IPv6" sub={s.shadow.ipv6 === "tunnel" ? "Blocked if your server lacks it" : undefined}>
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
      <List title={s.shadow.encrypt === "traffic" ? "DNS resolver" : "DNS inside the tunnel"}>
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
      <List>
        <ToggleRow label="Kill switch" sub="Blocks the internet if SHADOW drops, instead of leaking" checked={s.shadow.kill_switch} onChange={(v) => save((x) => ({ ...x, shadow: { ...x.shadow, kill_switch: v } }))} />
        <ToggleRow label="Allow local network" sub="Printers, shares and your router" checked={s.shadow.allow_lan} onChange={(v) => save((x) => ({ ...x, shadow: { ...x.shadow, allow_lan: v } }))} />
      </List>
      {hint}
    </Page>
  );
}

const SPLIT_HINT: Record<SplitMode, string> = {
  off: "All traffic follows your Connection settings.",
  bypass: "Listed apps and sites use your normal connection. Everything else uses the tunnel.",
  only: "Only listed apps and sites use the tunnel. Everything else uses your normal connection.",
};

/** A list of entries with remove buttons, an add field and optional quick-add suggestions. */
function EntryList({
  title,
  items,
  placeholder,
  normalize,
  invalid,
  hint,
  suggestions = [],
  onChange,
}: {
  title: string;
  items: string[];
  placeholder: string;
  normalize: (s: string) => string | null;
  invalid: string;
  hint?: string;
  suggestions?: string[];
  onChange: (items: string[]) => void;
}) {
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const add = (raw: string) => {
    const v = normalize(raw);
    if (!v) return setError(invalid);
    setError(null);
    setText("");
    if (!items.some((i) => i.toLowerCase() === v.toLowerCase())) onChange([...items, v]);
  };
  const fresh = suggestions.filter((s) => !items.some((i) => i.toLowerCase() === s.toLowerCase())).slice(0, 10);
  return (
    <section className="flex flex-col">
      <SectionTitle>{title}</SectionTitle>
      <Card className="p-1">
        {items.map((i) => (
          <SettingRow key={i} label={i}>
            <IconButton label={`Remove ${i}`} onClick={() => onChange(items.filter((x) => x !== i))}>
              <X className="size-4" />
            </IconButton>
          </SettingRow>
        ))}
        <form
          className={cn("flex items-center gap-2 p-1.5", items.length > 0 && "relative before:absolute before:top-0 before:right-3 before:left-3 before:h-px before:bg-border")}
          onSubmit={(e) => {
            e.preventDefault();
            add(text);
          }}
        >
          <Input
            className="h-8 flex-1"
            placeholder={placeholder}
            value={text}
            aria-label={`Add to ${title}`}
            aria-invalid={!!error}
            spellCheck={false}
            onChange={(e) => {
              setText(e.target.value);
              setError(null);
            }}
          />
          <Button type="submit" size="sm" variant="secondary" disabled={!text.trim()}>
            <Plus /> Add
          </Button>
        </form>
      </Card>
      {error ? <p className="m-0 mt-2 px-3 text-[12px] text-destructive">{error}</p> : hint && <Hint className="mt-2">{hint}</Hint>}
      {fresh.length > 0 && (
        <div className="mt-2 flex flex-wrap items-center gap-1.5 px-2" aria-label="Apps open now">
          <span className="pr-1 text-[12px] text-muted-foreground">Open now</span>
          {fresh.map((s) => (
            <button
              key={s}
              type="button"
              onClick={() => add(s)}
              className="inline-flex h-7 items-center gap-1 rounded-full bg-fill px-2.5 text-[12px] text-muted-foreground transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            >
              <Plus className="size-3" /> {s}
            </button>
          ))}
        </div>
      )}
    </section>
  );
}

export function SplitSettings({ nav }: { nav: Nav }) {
  const s = useStore((st) => st.settings)!;
  const hint = useReconnectHint("shadow");
  const split = s.shadow.split;
  const [running, setRunning] = useState<string[]>([]);
  useEffect(() => {
    api.runningApps().then(setRunning).catch(() => setRunning([]));
  }, []);
  const set = (fn: (x: SplitTunnel) => SplitTunnel) => save((x) => ({ ...x, shadow: { ...x.shadow, split: fn(x.shadow.split) } }));
  return (
    <Page title="Split tunnelling" onBack={nav.back}>
      <List hint={SPLIT_HINT[split.mode]}>
        <SettingRow label="Mode">
          <Seg<SplitMode>
            label="Split tunnelling"
            value={split.mode}
            onChange={(v) => set((x) => ({ ...x, mode: v }))}
            options={[
              { value: "off", label: "Off" },
              { value: "bypass", label: "Exclude" },
              { value: "only", label: "Only these" },
            ]}
          />
        </SettingRow>
      </List>
      {s.shadow.encrypt === "dns" && (
        <Notice icon={Info} tone="info">
          Connection is set to DNS only, so no traffic uses the tunnel and these lists have no effect.
        </Notice>
      )}
      <EntryList title="Apps" items={split.apps} placeholder="discord.exe" normalize={normalizeApp} invalid="Type a program name, like discord.exe." suggestions={running} onChange={(apps) => set((x) => ({ ...x, apps }))} />
      <EntryList
        title="Sites"
        items={split.sites}
        placeholder="netflix.com or 10.0.0.0/8"
        normalize={normalizeSite}
        invalid="Type a site like netflix.com, or an IP address or range."
        hint="A site includes its subdomains."
        onChange={(sites) => set((x) => ({ ...x, sites }))}
      />
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
        <SettingRow label="LOTUS service" sub={service.connected ? `v${status?.service_version ?? "?"}` : service.message ?? "Not reachable"}>
          <Pill tone={service.connected ? "good" : "bad"}>
            <span className="size-1.5 rounded-full bg-current" /> {service.connected ? "Running" : "Down"}
          </Pill>
        </SettingRow>
        <SettingRow label="Networking engine" sub={`sing-box ${engine}`} />
        <NavRow label="Engine log" onClick={() => nav.push({ name: "logs" })} />
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
        <Hint>Needs administrator approval.</Hint>
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
