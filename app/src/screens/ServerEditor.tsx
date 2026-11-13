import { useMemo, useState, type ReactNode } from "react";
import { Check, Eye, EyeOff, LoaderCircle } from "lucide-react";
import { Advanced, Hint, Page, Seg } from "@/components/Controls";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { api, errorText } from "@/lib/api";
import { setState, toast, useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import type { Protocol, ServerProfile, TlsConfig, Transport } from "@/lib/types";
import type { Nav } from "../App";

type Kind = "reality" | "vless-tls" | "trojan" | "shadowsocks" | "hysteria2" | "tuic" | "anytls" | "naive" | "wireguard";

const KINDS: { value: Kind; label: string; note: string }[] = [
  { value: "reality", label: "VLESS · REALITY", note: "Recommended. No domain needed; looks like a normal HTTPS visit." },
  { value: "hysteria2", label: "Hysteria2", note: "QUIC/UDP. Fast on lossy or long-distance links." },
  { value: "vless-tls", label: "VLESS · TLS", note: "Your own domain and certificate, optional WebSocket/gRPC." },
  { value: "trojan", label: "Trojan", note: "TLS with your own domain and certificate." },
  { value: "naive", label: "NaiveProxy", note: "Uses Chromium's own network stack for the most browser-like TLS." },
  { value: "anytls", label: "AnyTLS", note: "TLS with padding to blur traffic patterns." },
  { value: "tuic", label: "TUIC", note: "QUIC-based proxy." },
  { value: "shadowsocks", label: "Shadowsocks", note: "Use the 2022 ciphers. Simple, but easy to identify as a proxy." },
  { value: "wireguard", label: "WireGuard", note: "Very fast, but easy to identify and block." },
];

const SS_METHODS = ["2022-blake3-aes-128-gcm", "2022-blake3-aes-256-gcm", "2022-blake3-chacha20-poly1305", "aes-256-gcm", "aes-128-gcm", "chacha20-ietf-poly1305", "xchacha20-ietf-poly1305"];
const FINGERPRINTS = ["chrome", "firefox", "edge", "safari", "ios", "android", "randomized"];

interface Form {
  kind: Kind;
  name: string;
  server: string;
  port: string;
  uuid: string;
  password: string;
  username: string;
  sni: string;
  fingerprint: string;
  publicKey: string;
  shortId: string;
  vision: boolean;
  transport: "tcp" | "ws" | "grpc";
  path: string;
  hostHeader: string;
  serviceName: string;
  method: string;
  obfs: boolean;
  obfsPassword: string;
  pin: string;
  certificate: string;
  cc: string;
  quic: boolean;
  privateKey: string;
  peerKey: string;
  psk: string;
  addresses: string;
  mtu: string;
  keepalive: string;
}

const EMPTY: Form = {
  kind: "reality",
  name: "",
  server: "",
  port: "443",
  uuid: "",
  password: "",
  username: "",
  sni: "",
  fingerprint: "chrome",
  publicKey: "",
  shortId: "",
  vision: true,
  transport: "tcp",
  path: "/",
  hostHeader: "",
  serviceName: "",
  method: SS_METHODS[0],
  obfs: false,
  obfsPassword: "",
  pin: "",
  certificate: "",
  cc: "bbr",
  quic: false,
  privateKey: "",
  peerKey: "",
  psk: "",
  addresses: "",
  mtu: "",
  keepalive: "25",
};

function fromProfile(p: ServerProfile): Form {
  const f: Form = { ...EMPTY, name: p.name, server: p.protocol.server, port: String(p.protocol.port) };
  const tls = (t?: TlsConfig) => {
    f.sni = t?.server_name ?? "";
    f.fingerprint = t?.fingerprint ?? (f.kind === "reality" ? "chrome" : "");
    f.pin = (t?.public_key_sha256 ?? []).join(",");
    f.certificate = t?.certificate ?? "";
  };
  const tr = (t: Transport) => {
    f.transport = t.type === "ws" ? "ws" : t.type === "grpc" ? "grpc" : "tcp";
    if (t.type === "ws") {
      f.path = t.path;
      f.hostHeader = t.host ?? "";
    }
    if (t.type === "grpc") f.serviceName = t.service_name;
  };
  const x = p.protocol;
  switch (x.type) {
    case "vless":
      f.kind = x.tls?.reality ? "reality" : "vless-tls";
      f.uuid = x.uuid;
      f.vision = x.flow === "xtls-rprx-vision";
      tls(x.tls);
      f.publicKey = x.tls?.reality?.public_key ?? "";
      f.shortId = x.tls?.reality?.short_id ?? "";
      tr(x.transport);
      break;
    case "trojan":
      f.kind = "trojan";
      f.password = x.password;
      tls(x.tls);
      tr(x.transport);
      break;
    case "shadowsocks":
      f.kind = "shadowsocks";
      f.method = x.method;
      f.password = x.password;
      break;
    case "hysteria2":
      f.kind = "hysteria2";
      f.password = x.password;
      f.obfs = !!x.obfs;
      f.obfsPassword = x.obfs?.password ?? "";
      tls(x.tls);
      break;
    case "tuic":
      f.kind = "tuic";
      f.uuid = x.uuid;
      f.password = x.password;
      f.cc = x.congestion_control;
      tls(x.tls);
      break;
    case "anytls":
      f.kind = "anytls";
      f.password = x.password;
      tls(x.tls);
      break;
    case "naive":
      f.kind = "naive";
      f.username = x.username;
      f.password = x.password;
      f.quic = x.quic;
      tls(x.tls);
      break;
    case "wireguard":
      f.kind = "wireguard";
      f.privateKey = x.private_key;
      f.peerKey = x.peer_public_key;
      f.psk = x.pre_shared_key ?? "";
      f.addresses = x.local_address.join(", ");
      f.mtu = x.mtu ? String(x.mtu) : "";
      f.keepalive = x.keepalive != null ? String(x.keepalive) : "";
      break;
  }
  return f;
}

function toProtocol(f: Form): Protocol {
  const server = f.server.trim().replace(/^\[|\]$/g, "");
  const port = Number(f.port) || 0;
  const opt = (s: string) => (s.trim() ? s.trim() : undefined);
  const list = (s: string) => s.split(",").map((v) => v.trim()).filter(Boolean);
  const tls = (withFp: boolean): TlsConfig => ({
    server_name: opt(f.sni),
    fingerprint: withFp ? opt(f.fingerprint) : undefined,
    public_key_sha256: list(f.pin).length ? list(f.pin) : undefined,
    certificate: opt(f.certificate),
  });
  const transport = (): Transport =>
    f.transport === "ws" ? { type: "ws", path: f.path || "/", host: opt(f.hostHeader) } : f.transport === "grpc" ? { type: "grpc", service_name: f.serviceName.trim() } : { type: "tcp" };
  switch (f.kind) {
    case "reality":
      return {
        type: "vless",
        server,
        port,
        uuid: f.uuid.trim(),
        flow: f.vision ? "xtls-rprx-vision" : undefined,
        tls: { server_name: opt(f.sni), fingerprint: f.fingerprint || "chrome", reality: { public_key: f.publicKey.trim(), short_id: f.shortId.trim() } },
        transport: { type: "tcp" },
      };
    case "vless-tls":
      return { type: "vless", server, port, uuid: f.uuid.trim(), flow: f.vision && f.transport === "tcp" ? "xtls-rprx-vision" : undefined, tls: tls(true), transport: transport() };
    case "trojan":
      return { type: "trojan", server, port, password: f.password, tls: tls(true), transport: transport() };
    case "shadowsocks":
      return { type: "shadowsocks", server, port, method: f.method, password: f.password };
    case "hysteria2":
      return { type: "hysteria2", server, port, password: f.password, obfs: f.obfs ? { kind: "salamander", password: f.obfsPassword } : undefined, tls: tls(false) };
    case "tuic":
      return { type: "tuic", server, port, uuid: f.uuid.trim(), password: f.password, congestion_control: f.cc, tls: { ...tls(false), alpn: ["h3"] } };
    case "anytls":
      return { type: "anytls", server, port, password: f.password, tls: tls(true) };
    case "naive":
      return { type: "naive", server, port, username: f.username, password: f.password, quic: f.quic, tls: { server_name: opt(f.sni), certificate: opt(f.certificate) } };
    case "wireguard":
      return {
        type: "wireguard",
        server,
        port,
        private_key: f.privateKey.trim(),
        peer_public_key: f.peerKey.trim(),
        pre_shared_key: opt(f.psk),
        local_address: list(f.addresses),
        mtu: f.mtu ? Number(f.mtu) : undefined,
        keepalive: f.keepalive ? Number(f.keepalive) : undefined,
      };
  }
}

function FieldShell({ id, label, children }: { id?: string; label: string; children: ReactNode }) {
  return (
    <div className="grid gap-1.5">
      <Label htmlFor={id} className="pl-0.5 text-[12px] font-medium text-muted-foreground">
        {label}
      </Label>
      {children}
    </div>
  );
}

function Field({ id, label, value, onChange, placeholder, secret, mono }: { id: string; label: string; value: string; onChange: (v: string) => void; placeholder?: string; secret?: boolean; mono?: boolean }) {
  const [show, setShow] = useState(!secret);
  return (
    <FieldShell id={id} label={label}>
      <div className="relative">
        <Input
          id={id}
          type={show ? "text" : "password"}
          spellCheck={false}
          autoComplete="off"
          value={value}
          placeholder={placeholder}
          className={cn(mono && "font-mono text-[12px]", secret && "pr-10")}
          onChange={(e) => onChange(e.target.value)}
        />
        {secret && (
          <button
            type="button"
            className="absolute top-1/2 right-1.5 grid size-7 -translate-y-1/2 place-items-center rounded-full text-faint transition-colors hover:bg-fill hover:text-foreground"
            aria-label={show ? "Hide" : "Show"}
            onClick={() => setShow(!show)}
          >
            {show ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
          </button>
        )}
      </div>
    </FieldShell>
  );
}

function Choice({ id, label, value, onChange, options }: { id: string; label: string; value: string; onChange: (v: string) => void; options: { value: string; label: string }[] }) {
  return (
    <FieldShell id={id} label={label}>
      <Select value={value} onValueChange={onChange}>
        <SelectTrigger id={id} className="w-full">
          <SelectValue />
        </SelectTrigger>
        <SelectContent position="popper" sideOffset={6}>
          {options.map((o) => (
            <SelectItem key={o.value} value={o.value}>
              {o.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </FieldShell>
  );
}

function SwitchField({ label, sub, checked, onChange }: { label: string; sub?: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="flex cursor-pointer items-center gap-3 py-1">
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="text-[13.5px] font-medium">{label}</span>
        {sub && <span className="text-[12px] text-muted-foreground">{sub}</span>}
      </span>
      <Switch checked={checked} onCheckedChange={onChange} aria-label={label} />
    </label>
  );
}

// Radix Select can't hold an empty value, so "no fingerprint" gets a sentinel.
const NO_FP = "none";

export function ServerEditor({ nav, id }: { nav: Nav; id?: string }) {
  const profiles = useStore((s) => s.profiles);
  const existing = useMemo(() => profiles.find((p) => p.id === id), [profiles, id]);
  const [f, setF] = useState<Form>(() => (existing ? fromProfile(existing) : EMPTY));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const set = <K extends keyof Form>(k: K) => (v: Form[K]) => setF((x) => ({ ...x, [k]: v }));
  const k = f.kind;
  const tlsKinds: Kind[] = ["reality", "vless-tls", "trojan", "hysteria2", "tuic", "anytls", "naive"];

  async function submit() {
    setError(null);
    setBusy(true);
    const profile: ServerProfile = {
      id: existing?.id ?? "",
      name: f.name.trim() || f.server.trim(),
      protocol: toProtocol(f),
      created_at: existing?.created_at ?? 0,
    };
    try {
      setState({ profiles: await api.saveProfile(profile) });
      toast(existing ? "Server updated" : "Server added");
      nav.back();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Page title={existing ? "Edit server" : "New server"} onBack={nav.back}>
      {!existing && (
        <Card className="gap-2 p-4">
          <Choice
            id="kind"
            label="Protocol"
            value={k}
            onChange={(v) => setF({ ...EMPTY, name: f.name, server: f.server, kind: v as Kind, port: v === "wireguard" ? "51820" : "443" })}
            options={KINDS.map((o) => ({ value: o.value, label: o.label }))}
          />
          <Hint className="px-0.5">{KINDS.find((o) => o.value === k)?.note}</Hint>
        </Card>
      )}
      <Card className="gap-3 p-4">
        <Field id="name" label="Name" value={f.name} onChange={set("name")} placeholder="My VPS" />
        <div className="grid grid-cols-[1fr_96px] gap-2.5">
          <Field id="server" label="Server address" value={f.server} onChange={set("server")} placeholder="203.0.113.10" />
          <Field id="port" label="Port" value={f.port} onChange={(v) => set("port")(v.replace(/\D/g, "").slice(0, 5))} />
        </div>
        {(k === "reality" || k === "vless-tls" || k === "tuic") && <Field id="uuid" label="UUID" value={f.uuid} onChange={set("uuid")} secret mono />}
        {k === "naive" && <Field id="user" label="Username" value={f.username} onChange={set("username")} />}
        {["trojan", "shadowsocks", "hysteria2", "tuic", "anytls", "naive"].includes(k) && <Field id="pw" label="Password" value={f.password} onChange={set("password")} secret mono={k === "shadowsocks"} />}
        {k === "shadowsocks" && <Choice id="m" label="Cipher" value={f.method} onChange={set("method")} options={SS_METHODS.map((m) => ({ value: m, label: m }))} />}
        {k === "reality" && (
          <>
            <Field id="sni" label="Camouflage site (SNI)" value={f.sni} onChange={set("sni")} placeholder="www.example.com" />
            <Field id="pbk" label="REALITY public key" value={f.publicKey} onChange={set("publicKey")} mono />
            <Field id="sid" label="Short ID" value={f.shortId} onChange={set("shortId")} mono placeholder="hex, up to 16 characters" />
          </>
        )}
        {tlsKinds.includes(k) && k !== "reality" && <Field id="sni" label="TLS server name" value={f.sni} onChange={set("sni")} placeholder="Defaults to the server address" />}
        {k === "wireguard" && (
          <>
            <Field id="pk" label="Your private key" value={f.privateKey} onChange={set("privateKey")} secret mono />
            <Field id="peer" label="Server public key" value={f.peerKey} onChange={set("peerKey")} mono />
            <Field id="addr" label="Interface addresses" value={f.addresses} onChange={set("addresses")} placeholder="10.66.0.2/32, fd00:66::2/128" mono />
          </>
        )}
        {k === "hysteria2" && <SwitchField label="Salamander obfuscation" sub="Only if your server requires it" checked={f.obfs} onChange={set("obfs")} />}
        {k === "hysteria2" && f.obfs && <Field id="obfs" label="Obfuscation password" value={f.obfsPassword} onChange={set("obfsPassword")} secret />}
        {k === "naive" && <SwitchField label="Use QUIC" checked={f.quic} onChange={set("quic")} />}
        {error && <Hint className="px-0.5 text-destructive">{error}</Hint>}
        <Button size="lg" className="mt-1 w-full" disabled={busy || !f.server.trim()} onClick={submit}>
          {busy ? <LoaderCircle className="animate-spin" /> : <Check />} Save
        </Button>
      </Card>

      <Advanced>
        <Card className="gap-3 p-4">
          {(k === "reality" || k === "vless-tls" || k === "trojan" || k === "anytls") && (
            <Choice
              id="fp"
              label="Browser fingerprint (uTLS)"
              value={f.fingerprint || NO_FP}
              onChange={(v) => set("fingerprint")(v === NO_FP ? "" : v)}
              options={[...(k !== "reality" ? [{ value: NO_FP, label: "None (Go TLS)" }] : []), ...FINGERPRINTS.map((x) => ({ value: x, label: x }))]}
            />
          )}
          {(k === "reality" || (k === "vless-tls" && f.transport === "tcp")) && <SwitchField label="XTLS-Vision" sub="Hides TLS-in-TLS patterns" checked={f.vision} onChange={set("vision")} />}
          {(k === "vless-tls" || k === "trojan") && (
            <>
              <FieldShell label="Transport">
                <Seg
                  label="Transport"
                  value={f.transport}
                  onChange={set("transport")}
                  className="w-full"
                  options={[
                    { value: "tcp", label: "TCP" },
                    { value: "ws", label: "WebSocket" },
                    { value: "grpc", label: "gRPC" },
                  ]}
                />
              </FieldShell>
              {f.transport === "ws" && (
                <div className="grid grid-cols-2 gap-2.5">
                  <Field id="path" label="Path" value={f.path} onChange={set("path")} />
                  <Field id="hh" label="Host header" value={f.hostHeader} onChange={set("hostHeader")} />
                </div>
              )}
              {f.transport === "grpc" && <Field id="svc" label="Service name" value={f.serviceName} onChange={set("serviceName")} />}
            </>
          )}
          {k === "tuic" && (
            <FieldShell label="Congestion control">
              <Seg
                label="Congestion control"
                value={f.cc}
                onChange={set("cc")}
                className="w-full"
                options={[
                  { value: "bbr", label: "BBR" },
                  { value: "cubic", label: "CUBIC" },
                  { value: "new_reno", label: "NewReno" },
                ]}
              />
            </FieldShell>
          )}
          {["vless-tls", "trojan", "hysteria2", "tuic", "anytls"].includes(k) && <Field id="pin" label="Pinned public key (SHA-256, base64)" value={f.pin} onChange={set("pin")} mono placeholder="Optional" />}
          {["vless-tls", "trojan", "hysteria2", "tuic", "anytls", "naive"].includes(k) && (
            <FieldShell id="cert" label="Extra trusted certificate (PEM)">
              <Textarea id="cert" rows={3} className="font-mono text-[11.5px]" value={f.certificate} onChange={(e) => set("certificate")(e.target.value)} placeholder="-----BEGIN CERTIFICATE-----" />
            </FieldShell>
          )}
          {k === "wireguard" && (
            <>
              <Field id="psk" label="Pre-shared key" value={f.psk} onChange={set("psk")} secret mono />
              <div className="grid grid-cols-2 gap-2.5">
                <Field id="mtu" label="MTU" value={f.mtu} onChange={(v) => set("mtu")(v.replace(/\D/g, ""))} placeholder="1408" />
                <Field id="ka" label="Keepalive (s)" value={f.keepalive} onChange={(v) => set("keepalive")(v.replace(/\D/g, ""))} />
              </div>
            </>
          )}
          <Hint className="px-0.5">Certificates are always verified.</Hint>
        </Card>
      </Advanced>
    </Page>
  );
}
