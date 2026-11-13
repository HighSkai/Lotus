import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { CircleCheck, Download, LoaderCircle, RefreshCw, RotateCw, TriangleAlert } from "lucide-react";
import { Advanced, Hint, List, Page, ToggleRow } from "@/components/Controls";
import { ModeMark } from "@/components/ShadowMark";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { api, errorText } from "@/lib/api";
import { easeOut } from "@/lib/motion";
import { installUpdate, setState, toast, updateSettings, useStore } from "@/lib/store";
import type { UpdateInfo } from "@/lib/types";
import type { Nav } from "../App";

function ago(ms: number | null, now: number): string {
  if (!ms) return "";
  const m = Math.round((now - ms) / 60000);
  if (m < 1) return "Checked just now";
  if (m < 60) return `Checked ${m} min ago`;
  const h = Math.round(m / 60);
  if (h < 24) return `Checked ${h} h ago`;
  return `Checked ${new Date(ms).toLocaleDateString()}`;
}

/** Short status for the Settings list. */
export function updateLabel(u: UpdateInfo | null): string | undefined {
  if (!u) return undefined;
  if (u.phase === "ready") return `${u.version} ready`;
  if (u.phase === "available" || u.phase === "downloading") return "Update available";
  return u.current;
}

export function Updates({ nav }: { nav: Nav }) {
  const u = useStore((s) => s.update);
  const settings = useStore((s) => s.settings)!;
  const mode = settings.mode;
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), 30_000);
    return () => window.clearInterval(t);
  }, []);

  const phase = u?.phase ?? "idle";
  const busy = phase === "checking" || phase === "downloading" || phase === "installing";

  async function check() {
    try {
      setState({ update: await api.checkUpdate() });
    } catch (e) {
      toast(errorText(e), "error");
    }
  }

  const title =
    phase === "ready"
      ? `LOTUS ${u?.version} is ready`
      : phase === "available"
        ? `LOTUS ${u?.version} is available`
        : phase === "downloading"
          ? `Downloading ${u?.version}`
          : phase === "installing"
            ? "Starting the installer…"
            : phase === "checking"
              ? "Checking for updates…"
              : phase === "up_to_date"
                ? "You're up to date"
                : phase === "error"
                  ? "Couldn't check for updates"
                  : `LOTUS ${u?.current ?? ""}`;
  const sub =
    phase === "error"
      ? u?.error ?? ""
      : phase === "ready" || phase === "available"
        ? `You have ${u?.current}.`
        : phase === "up_to_date" || phase === "idle"
          ? [`Version ${u?.current ?? ""}`, ago(u?.checked_at ?? null, now)].filter(Boolean).join(" · ")
          : "";

  return (
    <Page title="Updates" onBack={nav.back}>
      <Card className="items-center gap-3 px-5 py-6 text-center">
        <div className="relative grid size-14 place-items-center">
          <ModeMark mode={mode} className="size-14" />
          <AnimatePresence>
            {phase === "up_to_date" && (
              <motion.span key="ok" initial={{ scale: 0, opacity: 0 }} animate={{ scale: 1, opacity: 1 }} exit={{ scale: 0, opacity: 0 }} className="absolute -right-1 -bottom-1 grid size-6 place-items-center rounded-full bg-good text-white ring-3 ring-[var(--bg-b)]">
                <CircleCheck className="size-4" />
              </motion.span>
            )}
            {phase === "error" && (
              <motion.span key="err" initial={{ scale: 0, opacity: 0 }} animate={{ scale: 1, opacity: 1 }} exit={{ scale: 0, opacity: 0 }} className="absolute -right-1 -bottom-1 grid size-6 place-items-center rounded-full bg-destructive text-white ring-3 ring-[var(--bg-b)]">
                <TriangleAlert className="size-3.5" />
              </motion.span>
            )}
          </AnimatePresence>
        </div>
        <AnimatePresence mode="wait" initial={false}>
          <motion.div key={title} initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -4 }} transition={{ duration: 0.18, ease: easeOut }} className="grid gap-1">
            <div className="text-[16px] font-semibold tracking-[-0.01em]">{title}</div>
            {sub && <div className={phase === "error" ? "text-[12.5px] text-destructive" : "text-[12.5px] text-muted-foreground"}>{sub}</div>}
          </motion.div>
        </AnimatePresence>

        {phase === "downloading" && (
          <div className="mt-1 h-1.5 w-full max-w-[240px] overflow-hidden rounded-full bg-fill-strong" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round((u?.progress ?? 0) * 100)}>
            <motion.div className="h-full rounded-full bg-linear-120 from-on-a to-on-c" initial={false} animate={{ width: `${Math.max(4, (u?.progress ?? 0) * 100)}%` }} transition={{ duration: 0.25, ease: easeOut }} />
          </div>
        )}

        <div className="mt-1 flex flex-wrap justify-center gap-2">
          {phase === "ready" ? (
            <Button onClick={() => void installUpdate()}>
              <RotateCw /> Restart to update
            </Button>
          ) : phase === "available" ? (
            <Button onClick={() => void installUpdate()}>
              <Download /> Download and install
            </Button>
          ) : (
            <Button variant="secondary" disabled={busy} onClick={check}>
              {busy ? <LoaderCircle className="animate-spin" /> : <RefreshCw />} {phase === "error" ? "Try again" : "Check for updates"}
            </Button>
          )}
        </div>
        {(phase === "ready" || phase === "available") && <Hint className="px-0">Protection pauses for a moment while LOTUS updates, then resumes by itself.</Hint>}
      </Card>

      {u?.notes && (phase === "ready" || phase === "available" || phase === "downloading") && (
        <Advanced label="What's new">
          <Card className="selectable p-4 text-[12.5px] leading-relaxed whitespace-pre-wrap text-muted-foreground select-text">{u.notes}</Card>
        </Advanced>
      )}

      <List hint="Updates are signed, and LOTUS checks the signature before installing. Windows asks for administrator approval.">
        <ToggleRow
          icon={Download}
          label="Update automatically"
          sub="Checks every few hours and downloads in the background. You choose when to restart."
          checked={settings.auto_update}
          onChange={(v) =>
            updateSettings((x) => ({ ...x, auto_update: v })).catch((e) => toast(errorText(e), "error"))
          }
        />
      </List>
    </Page>
  );
}
