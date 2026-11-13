import { useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import NumberFlow from "@number-flow/react";
import { Check, Gauge, Info, LoaderCircle, Play, TriangleAlert } from "lucide-react";
import { Hint, Page } from "@/components/Controls";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { api, errorText } from "@/lib/api";
import { easeOut } from "@/lib/motion";
import { useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import type { DiagnosticsReport, SpeedResult } from "@/lib/types";
import type { Nav } from "../App";

const OUTCOME = {
  pass: { icon: Check, cls: "bg-good/14 text-good" },
  warn: { icon: TriangleAlert, cls: "bg-warn/15 text-warn" },
  fail: { icon: TriangleAlert, cls: "bg-destructive/12 text-destructive" },
  info: { icon: Info, cls: "bg-primary/12 text-primary" },
} as const;

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
      <Card className="flex-row items-center gap-3 p-4">
        <div className="min-w-0 flex-1">
          <div className="text-[14px] font-semibold">{!report ? "Check for leaks" : fails ? `${fails} problem${fails > 1 ? "s" : ""} found` : warns ? "Mostly protected" : "No leaks found"}</div>
          <div className="mt-0.5 text-[12px] text-muted-foreground">
            {report ? `${new Date(report.generated_at).toLocaleTimeString()} · ${report.mode ? report.mode.toUpperCase() : "not connected"}` : "DNS, IPv4, IPv6 and routing"}
          </div>
        </div>
        <Button onClick={run} disabled={busy || !service.connected}>
          {busy ? <LoaderCircle className="animate-spin" /> : <Play />} {report ? "Run again" : "Run"}
        </Button>
      </Card>
      {err && <Hint className="text-destructive">{err}</Hint>}
      <AnimatePresence initial={false}>
        {report && (
          <motion.div key={report.generated_at} initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }} transition={{ duration: 0.22, ease: easeOut }}>
            <Card className="p-1">
              {report.checks.map((c, i) => {
                const o = OUTCOME[c.outcome];
                return (
                  <motion.div
                    key={c.id}
                    initial={{ opacity: 0, y: 6 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ duration: 0.22, ease: easeOut, delay: i * 0.04 }}
                    className="relative flex gap-3 px-3 py-3 not-first:before:absolute not-first:before:top-0 not-first:before:right-3 not-first:before:left-3 not-first:before:h-px not-first:before:bg-border"
                  >
                    <span className={cn("grid size-[26px] shrink-0 place-items-center rounded-[9px]", o.cls)}>
                      <o.icon className="size-[15px]" />
                    </span>
                    <div className="min-w-0">
                      <div className="text-[11.5px] text-faint">{c.title}</div>
                      <div className="text-[13px] font-medium">{c.summary}</div>
                      {c.detail && <div className="selectable mt-0.5 text-[12px] leading-snug text-muted-foreground select-text">{c.detail}</div>}
                    </div>
                  </motion.div>
                );
              })}
            </Card>
          </motion.div>
        )}
      </AnimatePresence>
    </Page>
  );
}

function Stat({ v, unit, digits = 1 }: { v: number | null | undefined; unit: string; digits?: number }) {
  return (
    <div className="flex-1 text-center">
      <div className="text-[22px] font-semibold tracking-[-0.02em] tabular-nums">
        {v == null ? "—" : <NumberFlow value={v} format={{ maximumFractionDigits: v >= 100 ? 0 : digits }} transformTiming={{ duration: 450, easing: "cubic-bezier(0.22, 1, 0.36, 1)" }} />}
      </div>
      <div className="text-[12px] text-muted-foreground">{unit}</div>
    </div>
  );
}

export function SpeedTest({ nav }: { nav: Nav }) {
  const [r, setR] = useState<SpeedResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const status = useStore((s) => s.status);
  const service = useStore((s) => s.service);
  const via = status?.state === "connected" ? (status.mode === "shadow" ? `through ${status.profile_name ?? "your server"}` : "with LOTUS DNS") : "on your normal connection";
  return (
    <Page title="Speed test" onBack={nav.back}>
      <Card className="flex-row items-start py-4">
        <Stat v={r?.download_mbps} unit="Mbps down" />
        <Stat v={r?.upload_mbps} unit="Mbps up" />
        <Stat v={r?.latency_ms} unit="ms latency" digits={0} />
      </Card>
      <Button
        size="lg"
        className="w-full"
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
            <LoaderCircle className="animate-spin" /> Measuring…
          </>
        ) : (
          <>
            <Gauge /> Start test
          </>
        )}
      </Button>
      {err && <Hint className="text-destructive">{err}</Hint>}
      <Hint>
        Runs {via}. {r?.message ?? "Uses up to about 60 MB."}
      </Hint>
    </Page>
  );
}
