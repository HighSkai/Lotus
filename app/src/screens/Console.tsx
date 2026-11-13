import { useEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { ArrowDown, ArrowUp, Check, Info, LoaderCircle, ShieldCheck, TriangleAlert } from "lucide-react";
import { Page } from "@/components/Controls";
import { ModeMark } from "@/components/ShadowMark";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { api, errorText } from "@/lib/api";
import { bytes, country, rate } from "@/lib/format";
import { easeOut } from "@/lib/motion";
import { getState, useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import type { ConnInfo, ConnectionsSnapshot, VerifyReport } from "@/lib/types";
import type { Nav } from "../App";

type Tone = "info" | "ok" | "warn" | "bad" | "open" | "close";
interface Line {
  id: number;
  at: number;
  tone: Tone;
  text: string;
}

const WINDOW = 60;
const OUTCOME = {
  pass: { icon: Check, cls: "bg-good/14 text-good", tone: "ok" as Tone },
  warn: { icon: TriangleAlert, cls: "bg-warn/15 text-warn", tone: "warn" as Tone },
  fail: { icon: TriangleAlert, cls: "bg-destructive/12 text-destructive", tone: "bad" as Tone },
  info: { icon: Info, cls: "bg-primary/12 text-primary", tone: "info" as Tone },
};
const TONE: Record<Tone, string> = {
  info: "text-[#b6b8d6]",
  ok: "text-good",
  warn: "text-warn",
  bad: "text-destructive",
  open: "text-primary",
  close: "text-[#8a8db0]",
};
const TAG: Record<Tone, string> = { info: "info", ok: "ok", warn: "warn", bad: "fail", open: "open", close: "closed" };
const clock = (t: number) => new Date(t).toLocaleTimeString([], { hour12: false });

function routeLabel(route: string, server: string, short = false) {
  if (route.includes("proxy")) return { text: short ? "tunnel" : `via ${server}`, cls: "text-good" };
  if (route.includes("direct")) return { text: "direct", cls: "text-warn" };
  if (route.includes("block") || route.includes("reject")) return { text: "blocked", cls: "text-destructive" };
  return { text: route || "engine", cls: "text-muted-foreground" };
}

/** Upload/download over the last minute. */
function Graph({ samples }: { samples: { up: number; down: number }[] }) {
  const max = Math.max(64 * 1024, ...samples.map((s) => Math.max(s.up, s.down)));
  const pts = (key: "up" | "down") =>
    samples.map((s, i) => `${((i + WINDOW - samples.length) / (WINDOW - 1)) * 100},${100 - (s[key] / max) * 92}`).join(" ");
  const down = pts("down");
  return (
    <svg viewBox="0 0 100 100" preserveAspectRatio="none" className="h-16 w-full" aria-hidden="true">
      {samples.length > 1 && (
        <>
          <polygon points={`${down.split(" ")[0].split(",")[0]},100 ${down} 100,100`} fill="var(--primary)" opacity="0.14" />
          <polyline points={down} fill="none" stroke="var(--primary)" strokeWidth="1.6" vectorEffect="non-scaling-stroke" strokeLinejoin="round" />
          <polyline points={pts("up")} fill="none" stroke="var(--accent-2)" strokeWidth="1.4" vectorEffect="non-scaling-stroke" strokeLinejoin="round" />
        </>
      )}
    </svg>
  );
}

export function Console({ nav }: { nav: Nav }) {
  const settings = useStore((s) => s.settings)!;
  const status = useStore((s) => s.status);
  const service = useStore((s) => s.service);
  const profiles = useStore((s) => s.profiles);
  const selected = profiles.find((p) => p.id === settings.selected_profile) ?? null;
  const live = !!status && status.mode === "shadow" && (status.state === "connected" || status.state === "reconnecting");
  const serverName = status?.profile_name ?? selected?.name ?? "your server";

  const [samples, setSamples] = useState<{ up: number; down: number }[]>([]);
  const [snap, setSnap] = useState<ConnectionsSnapshot | null>(null);
  const [lines, setLines] = useState<Line[]>([]);
  const [report, setReport] = useState<VerifyReport | null>(null);
  const [verifying, setVerifying] = useState(false);
  const seen = useRef(new Map<string, ConnInfo>());
  const seq = useRef(0);
  const logBox = useRef<HTMLDivElement>(null);

  const log = (tone: Tone, text: string) => setLines((l) => [...l.slice(-299), { id: ++seq.current, at: Date.now(), tone, text }]);

  // Session events.
  const prev = useRef<string | null>(null);
  useEffect(() => {
    const key = `${status?.mode}-${status?.state}`;
    if (prev.current === key) return;
    prev.current = key;
    if (!status) return log("warn", "LOTUS service not reachable");
    if (status.mode !== "shadow") return log("info", "SHADOW is off. Turn it on to see traffic through your server.");
    if (status.state === "connected") log("ok", `Connected to ${status.profile_name ?? "server"} · ${status.protocol_label ?? ""} · exit ${status.exit?.ipv4 ?? "?"}`);
    else if (status.state === "reconnecting") log("warn", "Reconnecting to the server…");
    else if (status.state === "connecting") log("info", "Connecting…");
    else if (status.state === "error") log("bad", status.error?.message ?? "Connection failed");
    else log("info", "Disconnected");
  }, [status?.mode, status?.state]); // eslint-disable-line react-hooks/exhaustive-deps

  // Throughput samples, once a second.
  useEffect(() => {
    const t = window.setInterval(() => {
      const s = getState().status;
      const on = s?.mode === "shadow" && s.state === "connected";
      setSamples((a) => [...a.slice(-(WINDOW - 1)), { up: on ? s!.traffic.up_rate : 0, down: on ? s!.traffic.down_rate : 0 }]);
    }, 1000);
    return () => window.clearInterval(t);
  }, []);

  // Live connections while connected.
  useEffect(() => {
    if (!live) {
      setSnap(null);
      seen.current.clear();
      return;
    }
    let stop = false;
    const poll = async () => {
      try {
        const s = await api.connections();
        if (stop) return;
        setSnap(s);
        const now = new Map(s.connections.map((c) => [c.id, c]));
        for (const c of s.connections) {
          if (!seen.current.has(c.id)) log("open", `${c.network} ${c.host}:${c.port} ${routeLabel(c.route, serverName).text}`);
        }
        for (const [id, c] of seen.current) {
          if (!now.has(id)) log("close", `${c.host}:${c.port}, ${bytes(c.upload)} up, ${bytes(c.download)} down`);
        }
        seen.current = now;
      } catch (e) {
        if (!stop) log("warn", `Couldn't read live connections: ${errorText(e)}`);
      }
    };
    poll();
    const t = window.setInterval(poll, 1000);
    return () => {
      stop = true;
      window.clearInterval(t);
    };
  }, [live]); // eslint-disable-line react-hooks/exhaustive-deps

  // Follow new lines only inside the log box, and only while it's scrolled to the end.
  useEffect(() => {
    const el = logBox.current;
    if (el && el.scrollHeight - el.scrollTop - el.clientHeight < 80) el.scrollTop = el.scrollHeight;
  }, [lines.length]);

  async function verify() {
    if (!selected) return;
    setVerifying(true);
    log("info", `Verifying ${selected.name}…`);
    try {
      const r = await api.verifyServer(selected.id);
      setReport(r);
      for (const c of r.checks) log(OUTCOME[c.outcome].tone, `${c.title}: ${c.summary}`);
    } catch (e) {
      log("bad", `Verify failed: ${errorText(e)}`);
    } finally {
      setVerifying(false);
    }
  }

  const top = useMemo(() => (snap?.connections ?? []).slice(0, 8), [snap]);
  const cur = samples[samples.length - 1] ?? { up: 0, down: 0 };

  return (
    <Page title="Live console" onBack={nav.back}>
      <Card className="gap-3 p-4">
        <div className="flex items-center gap-3">
          <ModeMark mode="shadow" className="size-10" />
          <div className="min-w-0 flex-1">
            <div className="truncate text-[14px] font-semibold">{selected?.name ?? "No server selected"}</div>
            <div className="truncate text-[12px] text-muted-foreground">{status?.protocol_label ?? report?.protocol ?? "SHADOW server"}</div>
          </div>
          <span className={cn("text-[12.5px] font-medium", live ? "text-good" : "text-faint")}>{live ? "Connected" : "Off"}</span>
        </div>
        {live && (
          <p className="m-0 truncate text-[12.5px] text-muted-foreground">
            <span className="font-mono text-foreground tabular-nums">{status?.exit?.ipv4 ?? "—"}</span>
            {status?.exit?.country && ` · ${country(status.exit.country)}`}
            {status?.latency_ms != null && ` · ${status.latency_ms} ms`}
            {` · ${snap?.connections.length ?? 0} open`}
          </p>
        )}
        <div>
          <Graph samples={samples} />
          <div className="mt-1 flex justify-between font-mono text-[12px] text-muted-foreground tabular-nums">
            <span className="inline-flex items-center gap-1">
              <ArrowUp className="size-3 text-accent-2" /> {rate(cur.up)}
            </span>
            <span>{snap ? `${bytes(snap.upload_total + snap.download_total)} this session` : "last 60 s"}</span>
            <span className="inline-flex items-center gap-1">
              <ArrowDown className="size-3 text-primary" /> {rate(cur.down)}
            </span>
          </div>
        </div>
        <Button variant="secondary" disabled={!selected || verifying || !service.connected} onClick={verify}>
          {verifying ? <LoaderCircle className="animate-spin" /> : <ShieldCheck />} {verifying ? "Checking the server…" : "Verify server"}
        </Button>
      </Card>

      <AnimatePresence initial={false}>
        {report && (
          <motion.div key={report.generated_at} initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }} transition={{ duration: 0.22, ease: easeOut }}>
            <Card className="p-1">
              {report.checks.map((c) => {
                const o = OUTCOME[c.outcome];
                return (
                  <div key={c.id} className="relative flex gap-3 px-3 py-3 not-first:before:absolute not-first:before:top-0 not-first:before:right-3 not-first:before:left-3 not-first:before:h-px not-first:before:bg-border">
                    <span className={cn("grid size-[26px] shrink-0 place-items-center rounded-[9px]", o.cls)}>
                      <o.icon className="size-[15px]" />
                    </span>
                    <div className="min-w-0">
                      <div className="text-[11.5px] text-faint">{c.title}</div>
                      <div className="text-[13px] font-medium">{c.summary}</div>
                      {c.detail && <div className="selectable mt-0.5 text-[12px] leading-snug text-muted-foreground select-text">{c.detail}</div>}
                    </div>
                  </div>
                );
              })}
            </Card>
          </motion.div>
        )}
      </AnimatePresence>

      {live && top.length > 0 && (
        <Card className="p-1">
          {top.map((c) => {
            const r = routeLabel(c.route, serverName, true);
            return (
              <div key={c.id} className="relative flex items-center gap-2 px-3 py-2 not-first:before:absolute not-first:before:top-0 not-first:before:right-3 not-first:before:left-3 not-first:before:h-px not-first:before:bg-border">
                <span className="w-8 shrink-0 font-mono text-[11.5px] text-muted-foreground uppercase">{c.network}</span>
                <span className="min-w-0 flex-1 truncate font-mono text-[12px]" title={`${c.host}:${c.port}`}>
                  {c.host}
                  <span className="text-muted-foreground">:{c.port}</span>
                </span>
                <span className={cn("shrink-0 text-[11.5px] font-medium", r.cls)}>{r.text}</span>
                <span className="w-[66px] shrink-0 text-right font-mono text-[11.5px] text-muted-foreground tabular-nums">{bytes(c.upload + c.download)}</span>
              </div>
            );
          })}
        </Card>
      )}

      <div ref={logBox} className="selectable max-h-[260px] min-h-[140px] overflow-y-auto rounded-[18px] bg-[#0b0c18]/90 p-3 font-mono text-[12px] leading-relaxed shadow-[inset_0_0_0_1px_rgb(255_255_255/0.06)] select-text">
        {lines.map((l) => (
          <div key={l.id} className="flex gap-2">
            <span className="shrink-0 text-[#8487a6]">{clock(l.at)}</span>
            <span className={cn("w-11 shrink-0", TONE[l.tone])}>{TAG[l.tone]}</span>
            <span className="min-w-0 break-words text-[#d4d6ee]">{l.text}</span>
          </div>
        ))}
      </div>
    </Page>
  );
}
