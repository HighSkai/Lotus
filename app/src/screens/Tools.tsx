import { useState } from "react";
import { Icon } from "../components/Icon";
import { Page } from "../components/Controls";
import { api, errorText } from "../lib/api";
import { useStore } from "../lib/store";
import type { DiagnosticsReport, SpeedResult } from "../lib/types";
import type { Nav } from "../App";

const OUTCOME_ICON = { pass: "check", warn: "alert", fail: "alert", info: "info" } as const;

export function Diagnostics({ nav }: { nav: Nav }) {
  const [report, setReport] = useState<DiagnosticsReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const service = useStore((s) => s.service);
  const fails = report?.checks.filter((c) => c.outcome === "fail").length ?? 0;
  const warns = report?.checks.filter((c) => c.outcome === "warn").length ?? 0;

  async function run() {
    setBusy(true);
    setErr(null);
    try {
      setReport(await api.diagnostics());
    } catch (e) {
      setErr(errorText(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Page title="Privacy check" onBack={nav.back}>
      <div className="card" style={{ padding: 16, display: "flex", alignItems: "center", gap: 14 }}>
        <div style={{ flex: 1 }}>
          <div style={{ fontWeight: 620, fontSize: 14 }}>
            {!report ? "Check for leaks" : fails ? `${fails} problem${fails > 1 ? "s" : ""} found` : warns ? "Mostly protected" : "No leaks found"}
          </div>
          <div style={{ color: "var(--text-2)", fontSize: 12.5, marginTop: 2 }}>
            {report ? `Checked ${new Date(report.generated_at).toLocaleTimeString()} · ${report.mode ? report.mode.toUpperCase() : "not connected"}` : "Tests DNS, IPv4, IPv6 and routing with real network requests."}
          </div>
        </div>
        <button className="btn primary" onClick={run} disabled={busy || !service.connected}>
          {busy ? <span className="spinner" /> : <Icon name="play" />} {report ? "Run again" : "Run"}
        </button>
      </div>
      {err && <div className="error-text">{err}</div>}
      {report && (
        <div className="card" style={{ marginTop: 12 }}>
          {report.checks.map((c) => (
            <div key={c.id} className={`check ${c.outcome}`}>
              <span className="badge">
                <Icon name={OUTCOME_ICON[c.outcome]} />
              </span>
              <div style={{ minWidth: 0 }}>
                <div className="title">{c.title}</div>
                <div className="summary">{c.summary}</div>
                {c.detail && <div className="detail">{c.detail}</div>}
              </div>
            </div>
          ))}
        </div>
      )}
      <p className="hint">Nothing is simulated: each line comes from a real query or request made just now through your current connection.</p>
    </Page>
  );
}

export function SpeedTest({ nav }: { nav: Nav }) {
  const [r, setR] = useState<SpeedResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const status = useStore((s) => s.status);
  const service = useStore((s) => s.service);
  const via = status?.state === "connected" ? (status.mode === "shadow" ? `through ${status.profile_name ?? "your server"}` : "with LOTUS DNS") : "on your normal connection";
  const fmt = (n: number | null | undefined) => (n == null ? "—" : n >= 100 ? n.toFixed(0) : n.toFixed(1));
  return (
    <Page title="Speed test" onBack={nav.back}>
      <div className="stat-grid">
        <div className="card stat">
          <div className="k">DOWNLOAD</div>
          <div className="v">{fmt(r?.download_mbps)}</div>
          <div className="u">Mbps</div>
        </div>
        <div className="card stat">
          <div className="k">UPLOAD</div>
          <div className="v">{fmt(r?.upload_mbps)}</div>
          <div className="u">Mbps</div>
        </div>
        <div className="card stat">
          <div className="k">LATENCY</div>
          <div className="v">{r?.latency_ms ?? "—"}</div>
          <div className="u">ms</div>
        </div>
      </div>
      <button
        className="btn primary block"
        style={{ marginTop: 12 }}
        disabled={busy || !service.connected}
        onClick={async () => {
          setBusy(true);
          setErr(null);
          try {
            setR(await api.speedTest());
          } catch (e) {
            setErr(errorText(e));
          } finally {
            setBusy(false);
          }
        }}
      >
        {busy ? (
          <>
            <span className="spinner" /> Measuring… about 20 seconds
          </>
        ) : (
          <>
            <Icon name="speed" /> Start test
          </>
        )}
      </button>
      {err && <div className="error-text">{err}</div>}
      <p className="hint">
        Runs {via}. {r?.message ?? "Uses up to about 60 MB of data and only runs when you press the button."}
      </p>
    </Page>
  );
}
