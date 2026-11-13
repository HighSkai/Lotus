import { useRef, useState } from "react";
import { Icon } from "../components/Icon";
import { LotusMark } from "../components/LotusMark";
import { Page } from "../components/Controls";
import { api, errorText } from "../lib/api";
import { hostLabel } from "../lib/format";
import { setState, toast, useStore } from "../lib/store";
import type { ImportResult, TestResult } from "../lib/types";
import { labelFor } from "./Home";
import type { Nav } from "../App";

export function Servers({ nav }: { nav: Nav }) {
  const profiles = useStore((s) => s.profiles);
  const settings = useStore((s) => s.settings)!;
  const mode = settings.mode;
  const [tests, setTests] = useState<Record<string, TestResult | "running">>({});

  async function test(id: string) {
    setTests((t) => ({ ...t, [id]: "running" }));
    try {
      const r = await api.testProfile(id);
      setTests((t) => ({ ...t, [id]: r }));
      if (!r.ok) toast(r.message, "error");
    } catch (e) {
      setTests((t) => {
        const n = { ...t };
        delete n[id];
        return n;
      });
      toast(errorText(e), "error");
    }
  }

  async function remove(id: string, name: string) {
    if (!window.confirm(`Delete “${name}”? Its keys are removed from this PC.`)) return;
    try {
      setState({ profiles: await api.deleteProfile(id) });
    } catch (e) {
      toast(errorText(e), "error");
    }
  }

  return (
    <Page
      title="Servers"
      onBack={nav.back}
      actions={
        <button className="btn small primary" onClick={() => nav.push({ name: "add-server" })}>
          <Icon name="plus" /> Add
        </button>
      }
    >
      {profiles.length === 0 ? (
        <div className="card empty">
          <LotusMark mode={mode} className="big" water />
          <strong style={{ color: "var(--text-1)" }}>Bring your own server</strong>
          <span>SHADOW sends your traffic through a server you control, so websites see its address instead of yours.</span>
          <div className="btn-row" style={{ justifyContent: "center" }}>
            <button className="btn primary" onClick={() => nav.push({ name: "add-server" })}>
              <Icon name="plus" /> Add a server
            </button>
            <button className="btn" onClick={() => nav.push({ name: "setup" })}>
              <Icon name="wand" /> Set one up
            </button>
          </div>
        </div>
      ) : (
        <div className="card list" role="radiogroup" aria-label="Exit server">
          {profiles.map((p) => {
            const t = tests[p.id];
            return (
              <div
                key={p.id}
                className="server"
                role="radio"
                tabIndex={0}
                aria-checked={settings.selected_profile === p.id}
                onClick={() => api.selectProfile(p.id).catch((e) => toast(errorText(e), "error"))}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    api.selectProfile(p.id).catch((er) => toast(errorText(er), "error"));
                  }
                }}
              >
                <span className="chip" style={{ width: 32, height: 32, borderRadius: 10, display: "grid", placeItems: "center", background: "color-mix(in srgb, var(--accent) 12%, transparent)", color: "var(--accent)" }}>
                  <Icon name="server" className="" />
                </span>
                <div className="meta">
                  <div className="name">{p.name}</div>
                  <div className="proto">
                    {labelFor(p.protocol.type)} · {hostLabel(p.protocol.server, p.protocol.port)}
                  </div>
                </div>
                {t === "running" ? (
                  <span className="spinner" style={{ color: "var(--accent)" }} />
                ) : t ? (
                  <span className={`pill ${t.ok ? "good" : "bad"}`} title={t.message}>
                    {t.ok ? `${t.latency_ms} ms` : "Failed"}
                  </span>
                ) : null}
                <div className="actions" onClick={(e) => e.stopPropagation()}>
                  <button className="icon-btn" aria-label={`Test ${p.name}`} title="Test" onClick={() => test(p.id)}>
                    <Icon name="pulse" />
                  </button>
                  <button className="icon-btn" aria-label={`Copy link for ${p.name}`} title="Copy share link" onClick={() => api.copyShareLink(p.id).then(() => toast("Link copied. It contains your server's secret"))}>
                    <Icon name="copy" />
                  </button>
                  <button className="icon-btn" aria-label={`Edit ${p.name}`} title="Edit" onClick={() => nav.push({ name: "edit-server", id: p.id })}>
                    <Icon name="edit" />
                  </button>
                  <button className="icon-btn" aria-label={`Delete ${p.name}`} title="Delete" onClick={() => remove(p.id, p.name)}>
                    <Icon name="trash" />
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
      {profiles.length > 0 && <p className="hint">Tap a server to use it. Testing performs a real secure handshake without changing your routing.</p>}
    </Page>
  );
}

export function AddServer({ nav }: { nav: Nav }) {
  const [text, setText] = useState("");
  const [result, setResult] = useState<ImportResult | null>(null);
  const [busy, setBusy] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);

  async function run(p: Promise<ImportResult>) {
    setBusy(true);
    try {
      const r = await p;
      setResult(r);
      setState({ profiles: r.profiles });
      if (r.added.length) {
        toast(r.added.length === 1 ? `Added “${r.added[0].name}”` : `Added ${r.added.length} servers`);
        if (!r.issues.length) nav.replace({ name: "servers" });
      }
    } catch (e) {
      toast(errorText(e), "error");
    } finally {
      setBusy(false);
    }
  }

  return (
    <Page title="Add server" onBack={nav.back}>
      <div className="card" style={{ padding: 14 }}>
        <div className="field" style={{ marginBottom: 10 }}>
          <label htmlFor="imp">Paste a link or configuration</label>
          <textarea
            id="imp"
            className="input"
            rows={5}
            spellCheck={false}
            placeholder={"lotus://…  vless://…  ss://…  trojan://…\nhysteria2://…  a WireGuard .conf  or sing-box JSON"}
            value={text}
            onChange={(e) => setText(e.target.value)}
          />
        </div>
        <div className="btn-row">
          <button className="btn primary" disabled={busy || !text.trim()} onClick={() => run(api.importText(text))}>
            {busy ? <span className="spinner" /> : <Icon name="check" />} Import
          </button>
          <button className="btn" disabled={busy} onClick={() => run(api.importClipboard())}>
            <Icon name="clipboard" /> From clipboard
          </button>
          <button className="btn" disabled={busy} onClick={() => fileRef.current?.click()}>
            <Icon name="file" /> File…
          </button>
          <input
            ref={fileRef}
            type="file"
            accept=".conf,.json,.txt,.yaml,.yml"
            hidden
            onChange={async (e) => {
              const f = e.target.files?.[0];
              e.target.value = "";
              if (!f) return;
              if (f.size > 2 * 1024 * 1024) {
                toast("That file is too large to be a server configuration.", "error");
                return;
              }
              run(api.importText(await f.text()));
            }}
          />
        </div>
      </div>
      {result && result.issues.length > 0 && (
        <div className="card" style={{ marginTop: 12 }}>
          {result.issues.map((i, n) => (
            <div className="check fail" key={n}>
              <span className="badge">
                <Icon name="alert" />
              </span>
              <div>
                <div className="title">{i.source}</div>
                <div className="detail" style={{ color: "var(--text-1)" }}>
                  {i.message}
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
      <div className="card list" style={{ marginTop: 12 }}>
        <button className="row" onClick={() => nav.push({ name: "edit-server" })}>
          <span className="chip">
            <Icon name="edit" />
          </span>
          <span className="text">
            <span className="value" style={{ fontWeight: 540 }}>
              Enter details manually
            </span>
            <span className="sub">VLESS, Trojan, Shadowsocks, Hysteria2, TUIC, AnyTLS, Naive, WireGuard</span>
          </span>
          <span className="trail">
            <Icon name="chevron" />
          </span>
        </button>
        <button className="row" onClick={() => nav.push({ name: "setup" })}>
          <span className="chip">
            <Icon name="wand" />
          </span>
          <span className="text">
            <span className="value" style={{ fontWeight: 540 }}>
              Set up my own server
            </span>
            <span className="sub">One script for a Debian or Ubuntu VPS</span>
          </span>
          <span className="trail">
            <Icon name="chevron" />
          </span>
        </button>
      </div>
      <p className="hint">LOTUS refuses links that turn off certificate verification or use unencrypted transports, and tells you why.</p>
    </Page>
  );
}

function Code({ text }: { text: string }) {
  return (
    <div className="code">
      {text}
      <button className="icon-btn" aria-label="Copy" onClick={() => api.copyText(text).then(() => toast("Copied"))}>
        <Icon name="copy" />
      </button>
    </div>
  );
}

export function ServerSetup({ nav }: { nav: Nav }) {
  const [saved, setSaved] = useState<string | null>(null);
  return (
    <Page title="Set up your server" onBack={nav.back}>
      <p className="hint" style={{ marginTop: 0 }}>
        The installer sets up sing-box on your VPS with VLESS + REALITY + Vision. It needs no domain name or certificate, and its handshake looks like an ordinary visit to a well-known HTTPS site. Optionally it adds Hysteria2 for lossy networks. Takes about a minute.
      </p>
      <div className="steps">
        <div className="card step">
          <h3>Get a VPS</h3>
          <p>Any provider works. Choose Debian 12/13 or Ubuntu 22.04/24.04, 512 MB RAM or more, located where you want your exit IP to be. Note its IP address and root (or sudo) login.</p>
        </div>
        <div className="card step">
          <h3>Save the installer</h3>
          <p>It verifies the sing-box download against a pinned SHA-256 digest, asks before touching existing firewall rules, and can be fully removed with --uninstall.</p>
          <div className="btn-row">
            <button
              className="btn primary"
              onClick={async () => {
                try {
                  const p = await api.saveServerScript();
                  if (p) setSaved(p);
                } catch (e) {
                  toast(errorText(e), "error");
                }
              }}
            >
              <Icon name="file" /> Save install script…
            </button>
          </div>
          {saved && <p style={{ marginTop: 8, marginBottom: 0 }}>Saved to {saved}</p>}
        </div>
        <div className="card step">
          <h3>Run it on the server</h3>
          <p>From PowerShell on this PC (replace YOUR_SERVER_IP):</p>
          <Code text={"scp lotus-server-install.sh root@YOUR_SERVER_IP:\nssh root@YOUR_SERVER_IP \"bash lotus-server-install.sh\""} />
          <p style={{ marginTop: 10 }}>Add Hysteria2 (UDP) as a second option:</p>
          <Code text={'ssh root@YOUR_SERVER_IP "bash lotus-server-install.sh --with-hysteria2"'} />
        </div>
        <div className="card step">
          <h3>Import the result</h3>
          <p>The script ends by printing a lotus:// link. Copy it, then:</p>
          <button className="btn primary" onClick={() => nav.replace({ name: "add-server" })}>
            <Icon name="clipboard" /> Paste the link
          </button>
        </div>
      </div>
      <p className="hint">Already run your own Xray or sing-box server? Skip this and paste its vless://, trojan://, ss:// or hysteria2:// link instead.</p>
    </Page>
  );
}
