import { useMemo, useState } from "react";
import { Icon } from "../components/Icon";
import { Page, Seg, Toggle } from "../components/Controls";
import { api, errorText } from "../lib/api";
import { setState, toast, useStore } from "../lib/store";
import type { Protocol, ServerProfile, TlsConfig, Transport } from "../lib/types";
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

function Field({ id, label, value, onChange, placeholder, secret, mono }: { id: string; label: string; value: string; onChange: (v: string) => void; placeholder?: string; secret?: boolean; mono?: boolean }) {
  const [show, setShow] = useState(!secret);
  return (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      <div style={{ position: "relative" }}>
        <input
          id={id}
          className="input"
          type={show ? "text" : "password"}
          spellCheck={false}
          autoComplete="off"
          value={value}
          placeholder={placeholder}
          style={mono ? { fontFamily: "var(--mono)", fontSize: 12, paddingRight: secret ? 40 : undefined } : secret ? { paddingRight: 40 } : undefined}
          onChange={(e) => onChange(e.target.value)}
        />
        {secret && (
          <button type="button" className="icon-btn" style={{ position: "absolute", right: 3, top: 2, width: 32, height: 32 }} aria-label={show ? "Hide" : "Show"} onClick={() => setShow(!show)}>
            <Icon name="eye" />
          </button>
        )}
      </div>
    </div>
  );
}

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
        <div className="card list" style={{ marginBottom: 12 }}>
          <div className="field" style={{ padding: "10px 10px 0" }}>
            <label htmlFor="kind">Protocol</label>
            <select id="kind" className="input" value={k} onChange={(e) => setF({ ...EMPTY, name: f.name, server: f.server, kind: e.target.value as Kind, port: e.target.value === "wireguard" ? "51820" : "443" })}>
              {KINDS.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </select>
          </div>
          <p className="hint" style={{ margin: "0 12px 10px" }}>
            {KINDS.find((o) => o.value === k)?.note}
          </p>
        </div>
      )}
      <div className="card" style={{ padding: 14 }}>
        <Field id="name" label="Name" value={f.name} onChange={set("name")} placeholder="My VPS" />
        <div className="field-row">
          <Field id="server" label="Server address" value={f.server} onChange={set("server")} placeholder="203.0.113.10 or vps.example.com" />
          <Field id="port" label="Port" value={f.port} onChange={(v) => set("port")(v.replace(/\D/g, "").slice(0, 5))} />
        </div>
        {(k === "reality" || k === "vless-tls" || k === "tuic") && <Field id="uuid" label="UUID" value={f.uuid} onChange={set("uuid")} secret mono />}
        {k === "naive" && <Field id="user" label="Username" value={f.username} onChange={set("username")} />}
        {["trojan", "shadowsocks", "hysteria2", "tuic", "anytls", "naive"].includes(k) && <Field id="pw" label="Password" value={f.password} onChange={set("password")} secret mono={k === "shadowsocks"} />}
        {k === "shadowsocks" && (
          <div className="field">
            <label htmlFor="m">Cipher</label>
            <select id="m" className="input" value={f.method} onChange={(e) => set("method")(e.target.value)}>
              {SS_METHODS.map((m) => (
                <option key={m}>{m}</option>
              ))}
            </select>
          </div>
        )}
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
        {k === "hysteria2" && (
          <div className="row" style={{ padding: "4px 2px", minHeight: 40 }}>
            <span className="text">
              <span className="value" style={{ fontWeight: 540 }}>
                Salamander obfuscation
              </span>
              <span className="sub">Only if your server requires it</span>
            </span>
            <Toggle checked={f.obfs} onChange={set("obfs")} label="Salamander obfuscation" />
          </div>
        )}
        {k === "hysteria2" && f.obfs && <Field id="obfs" label="Obfuscation password" value={f.obfsPassword} onChange={set("obfsPassword")} secret />}
        {k === "naive" && (
          <div className="row" style={{ padding: "4px 2px", minHeight: 40 }}>
            <span className="text">
              <span className="value" style={{ fontWeight: 540 }}>
                Use QUIC
              </span>
            </span>
            <Toggle checked={f.quic} onChange={set("quic")} label="Use QUIC" />
          </div>
        )}
        {error && <div className="error-text">{error}</div>}
        <button className="btn primary block" style={{ marginTop: 6 }} disabled={busy || !f.server.trim()} onClick={submit}>
          {busy ? <span className="spinner" /> : <Icon name="check" />} Save
        </button>
      </div>

      <details className="advanced">
        <summary>
          <Icon name="chevron" /> Advanced
        </summary>
        <div className="card" style={{ padding: 14 }}>
          {(k === "reality" || k === "vless-tls" || k === "trojan" || k === "anytls") && (
            <div className="field">
              <label htmlFor="fp">Browser fingerprint (uTLS)</label>
              <select id="fp" className="input" value={f.fingerprint} onChange={(e) => set("fingerprint")(e.target.value)}>
                {k !== "reality" && <option value="">None (Go TLS)</option>}
                {FINGERPRINTS.map((x) => (
                  <option key={x}>{x}</option>
                ))}
              </select>
            </div>
          )}
          {(k === "reality" || (k === "vless-tls" && f.transport === "tcp")) && (
            <div className="row" style={{ padding: "4px 2px", minHeight: 40, marginBottom: 8 }}>
              <span className="text">
                <span className="value" style={{ fontWeight: 540 }}>
                  XTLS-Vision
                </span>
                <span className="sub">Hides TLS-in-TLS patterns and avoids double encryption</span>
              </span>
              <Toggle checked={f.vision} onChange={set("vision")} label="XTLS-Vision" />
            </div>
          )}
          {(k === "vless-tls" || k === "trojan") && (
            <>
              <div className="field">
                <label>Transport</label>
                <Seg
                  label="Transport"
                  value={f.transport}
                  onChange={set("transport")}
                  options={[
                    { value: "tcp", label: "TCP" },
                    { value: "ws", label: "WebSocket" },
                    { value: "grpc", label: "gRPC" },
                  ]}
                />
              </div>
              {f.transport === "ws" && (
                <div className="field-row">
                  <Field id="path" label="Path" value={f.path} onChange={set("path")} />
                  <Field id="hh" label="Host header" value={f.hostHeader} onChange={set("hostHeader")} />
                </div>
              )}
              {f.transport === "grpc" && <Field id="svc" label="Service name" value={f.serviceName} onChange={set("serviceName")} />}
            </>
          )}
          {k === "tuic" && (
            <div className="field">
              <label>Congestion control</label>
              <Seg
                label="Congestion control"
                value={f.cc}
                onChange={set("cc")}
                options={[
                  { value: "bbr", label: "BBR" },
                  { value: "cubic", label: "CUBIC" },
                  { value: "new_reno", label: "NewReno" },
                ]}
              />
            </div>
          )}
          {["vless-tls", "trojan", "hysteria2", "tuic", "anytls"].includes(k) && (
            <Field id="pin" label="Pinned public key (SHA-256, base64)" value={f.pin} onChange={set("pin")} mono placeholder="Optional: trust exactly this server key" />
          )}
          {["vless-tls", "trojan", "hysteria2", "tuic", "anytls", "naive"].includes(k) && (
            <div className="field">
              <label htmlFor="cert">Extra trusted certificate (PEM)</label>
              <textarea id="cert" className="input" rows={3} value={f.certificate} onChange={(e) => set("certificate")(e.target.value)} placeholder="-----BEGIN CERTIFICATE-----" />
            </div>
          )}
          {k === "wireguard" && (
            <>
              <Field id="psk" label="Pre-shared key" value={f.psk} onChange={set("psk")} secret mono />
              <div className="field-row">
                <Field id="mtu" label="MTU" value={f.mtu} onChange={(v) => set("mtu")(v.replace(/\D/g, ""))} placeholder="1408" />
                <Field id="ka" label="Keepalive (s)" value={f.keepalive} onChange={(v) => set("keepalive")(v.replace(/\D/g, ""))} />
              </div>
            </>
          )}
          <p className="hint" style={{ margin: 0 }}>
            Certificates are always verified. There is deliberately no “allow insecure” option.
          </p>
        </div>
      </details>
    </Page>
  );
}
