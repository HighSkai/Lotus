import { useEffect, useMemo, useState } from "react";
import { List, Page, Seg, SettingRow, ToggleRow } from "@/components/Controls";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { api, errorText } from "@/lib/api";
import { bytes } from "@/lib/format";
import { toast, updateSettings, useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import type { UsageDay } from "@/lib/types";
import type { Nav } from "../App";

type Range = "week" | "month" | "year";
type Measure = "data" | "time";

const pad = (n: number) => String(n).padStart(2, "0");
const dayKey = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
const monthKey = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}`;

export function hours(seconds: number): string {
  if (seconds < 60) return seconds ? "under 1 min" : "0 min";
  const m = Math.round(seconds / 60);
  if (m < 60) return `${m} min`;
  const h = Math.floor(m / 60);
  return m % 60 ? `${h} h ${m % 60} min` : `${h} h`;
}

interface Bucket {
  key: string;
  label: string;
  title: string;
  bytes: number;
  seconds: number;
}

/** Buckets for the chosen range, oldest first; empty days stay in as zero. */
function buckets(days: UsageDay[], range: Range): Bucket[] {
  const now = new Date();
  const out: Bucket[] = [];
  if (range === "year") {
    for (let i = 11; i >= 0; i--) {
      const d = new Date(now.getFullYear(), now.getMonth() - i, 1);
      out.push({ key: monthKey(d), label: d.toLocaleDateString([], { month: "narrow" }), title: d.toLocaleDateString([], { month: "long", year: "numeric" }), bytes: 0, seconds: 0 });
    }
  } else {
    const n = range === "week" ? 7 : 30;
    for (let i = n - 1; i >= 0; i--) {
      const d = new Date(now.getFullYear(), now.getMonth(), now.getDate() - i);
      const label = range === "week" ? d.toLocaleDateString([], { weekday: "narrow" }) : i % 7 === 0 ? String(d.getDate()) : "";
      out.push({ key: dayKey(d), label, title: d.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" }), bytes: 0, seconds: 0 });
    }
  }
  const index = new Map(out.map((b) => [b.key, b]));
  for (const d of days) {
    const b = index.get(range === "year" ? d.date.slice(0, 7) : d.date);
    if (!b) continue;
    b.seconds += d.seconds;
    if (d.mode === "shadow") b.bytes += d.up + d.down;
  }
  return out;
}

/** Totals per server (and LOTUS) inside the range, largest first. */
function perServer(days: UsageDay[], keys: Set<string>, range: Range) {
  const rows = new Map<string, { name: string; bytes: number; seconds: number }>();
  for (const d of days) {
    if (!keys.has(range === "year" ? d.date.slice(0, 7) : d.date)) continue;
    const id = d.mode === "lotus" ? "lotus" : `s:${d.server_id ?? d.server ?? "?"}`;
    const r = rows.get(id) ?? { name: d.mode === "lotus" ? "LOTUS (encrypted DNS)" : d.server ?? "Server", bytes: 0, seconds: 0 };
    r.bytes += d.mode === "shadow" ? d.up + d.down : 0;
    r.seconds += d.seconds;
    if (d.server) r.name = d.mode === "lotus" ? r.name : d.server;
    rows.set(id, r);
  }
  return [...rows.values()].sort((a, b) => b.bytes - a.bytes || b.seconds - a.seconds);
}

function Chart({ data, measure }: { data: Bucket[]; measure: Measure }) {
  const [hover, setHover] = useState<number | null>(null);
  const value = (b: Bucket) => (measure === "data" ? b.bytes : b.seconds);
  const fmt = (v: number) => (measure === "data" ? bytes(v) : hours(v));
  const max = Math.max(...data.map(value), 1);
  const total = data.reduce((a, b) => a + value(b), 0);
  const h = hover != null ? data[hover] : null;
  return (
    <div>
      <div className="flex h-5 items-baseline justify-between text-[12.5px]">
        <span className="text-muted-foreground">{h ? h.title : "Total"}</span>
        <span className="font-medium tabular-nums">{fmt(h ? value(h) : total)}</span>
      </div>
      <div className="relative mt-2 h-[120px] border-b border-border" onMouseLeave={() => setHover(null)}>
        <div className="pointer-events-none absolute inset-x-0 top-0 border-t border-dashed border-border" />
        <span className="pointer-events-none absolute -top-2 right-0 w-11 text-right text-[11px] text-muted-foreground tabular-nums">{fmt(max === 1 && total === 0 ? 0 : max)}</span>
        <div className="flex h-full items-end gap-[2px] pr-12">
          {data.map((b, i) => (
            <button
              key={b.key}
              type="button"
              aria-label={`${b.title}: ${fmt(value(b))}`}
              onMouseEnter={() => setHover(i)}
              onFocus={() => setHover(i)}
              onBlur={() => setHover(null)}
              className="group flex h-full flex-1 items-end outline-none"
            >
              <span
                className={cn("w-full rounded-t-[4px] bg-[var(--chart)] transition-[height,opacity] duration-200 ease-out group-focus-visible:ring-2 group-focus-visible:ring-ring/50", hover != null && hover !== i && "opacity-45")}
                style={{ height: value(b) ? `max(${(value(b) / max) * 100}%, 3px)` : "0px" }}
              />
            </button>
          ))}
        </div>
      </div>
      <div className="mt-1 flex gap-[2px] pr-12" aria-hidden="true">
        {data.map((b) => (
          <span key={b.key} className="flex-1 text-center text-[11px] text-muted-foreground">
            {b.label}
          </span>
        ))}
      </div>
    </div>
  );
}

export function Usage({ nav }: { nav: Nav }) {
  const settings = useStore((s) => s.settings)!;
  const [days, setDays] = useState<UsageDay[] | null>(null);
  const [range, setRange] = useState<Range>("week");
  const [measure, setMeasure] = useState<Measure>("data");
  const [confirm, setConfirm] = useState(false);
  const recording = settings.lotus.record_usage && settings.shadow.record_usage;

  useEffect(() => {
    api.usage().then((r) => setDays(r.days), (e) => (setDays([]), toast(errorText(e), "error")));
  }, []);

  const data = useMemo(() => buckets(days ?? [], range), [days, range]);
  const rows = useMemo(() => perServer(days ?? [], new Set(data.map((b) => b.key)), range), [days, data, range]);
  const empty = days != null && days.length === 0;

  async function clear() {
    try {
      await api.clearUsage();
      setDays([]);
      toast("Usage history cleared");
    } catch (e) {
      toast(errorText(e), "error");
    }
  }

  return (
    <Page title="Usage" onBack={nav.back}>
      <div className="flex gap-2">
        <Seg<Range>
          label="Range"
          value={range}
          onChange={setRange}
          className="flex-1"
          options={[
            { value: "week", label: "Week" },
            { value: "month", label: "Month" },
            { value: "year", label: "Year" },
          ]}
        />
        <Seg<Measure>
          label="Measure"
          value={measure}
          onChange={setMeasure}
          options={[
            { value: "data", label: "Data" },
            { value: "time", label: "Time" },
          ]}
        />
      </div>
      <Card className="p-4">
        {empty ? (
          <p className="m-0 py-8 text-center text-[12.5px] text-muted-foreground">{recording ? "Time and data show up here after you connect." : "Usage history is off."}</p>
        ) : (
          <Chart data={data} measure={measure} />
        )}
      </Card>
      {rows.length > 0 && (
        <List>
          {rows.map((r) => (
            <SettingRow key={r.name} label={r.name}>
              <span className="tabular-nums">{r.bytes ? `${bytes(r.bytes)} · ` : ""}{hours(r.seconds)}</span>
            </SettingRow>
          ))}
        </List>
      )}
      <List hint="Kept on this PC only: totals per day and server, never the sites you visit.">
        <ToggleRow
          label="Keep usage history"
          checked={recording}
          onChange={(v) => updateSettings((x) => ({ ...x, lotus: { ...x.lotus, record_usage: v }, shadow: { ...x.shadow, record_usage: v } })).catch((e) => toast(errorText(e), "error"))}
        />
      </List>
      <Button variant="destructive" className="self-start" disabled={empty} onClick={() => setConfirm(true)}>
        Clear history
      </Button>
      <AlertDialog open={confirm} onOpenChange={setConfirm}>
        <AlertDialogContent size="sm">
          <AlertDialogHeader>
            <AlertDialogTitle>Clear usage history?</AlertDialogTitle>
            <AlertDialogDescription>All recorded days are deleted.</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction variant="destructive" onClick={clear}>
              Clear
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Page>
  );
}
