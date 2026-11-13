import type { ReactNode } from "react";
import { AnimatePresence, motion } from "motion/react";
import NumberFlow, { NumberFlowGroup } from "@number-flow/react";
import { cn } from "@/lib/utils";
import { easeOut } from "@/lib/motion";
import { byteParts, bytes, clockParts, duration } from "@/lib/format";
import type { LastSession } from "@/lib/store";
import type { Mode, Status } from "@/lib/types";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { Visual } from "./PowerSwitch";

// Short, smooth digit rolls: counters tick every second, so they must settle well inside that.
const flow = {
  transformTiming: { duration: 420, easing: "cubic-bezier(0.22, 1, 0.36, 1)" },
  spinTiming: { duration: 420, easing: "cubic-bezier(0.22, 1, 0.36, 1)" },
  opacityTiming: { duration: 220, easing: "ease-out" },
} as const;

function Clock({ ms, animated }: { ms: number; animated: boolean }) {
  const { h, m, s } = clockParts(ms);
  const two = { minimumIntegerDigits: 2 } as const;
  return (
    <NumberFlowGroup>
      <span className="inline-flex items-baseline">
        {h > 0 && (
          <>
            <NumberFlow value={h} trend={1} animated={animated} {...flow} />
            <span className="px-px">:</span>
          </>
        )}
        <NumberFlow value={m} format={two} digits={{ 1: { max: 5 } }} trend={1} animated={animated} {...flow} />
        <span className="px-px">:</span>
        <NumberFlow value={s} format={two} digits={{ 1: { max: 5 } }} trend={1} animated={animated} {...flow} />
      </span>
    </NumberFlowGroup>
  );
}

function Stat({ label, children, tip }: { label: string; children: ReactNode; tip?: ReactNode }) {
  const body = (
    <div className="flex min-w-[92px] flex-1 flex-col items-center gap-1 px-3">
      <div className="flex h-5 items-center text-[17px] leading-none font-semibold tracking-[-0.02em] tabular-nums">{children}</div>
      <div className="text-[10.5px] font-medium tracking-[0.07em] text-faint uppercase">{label}</div>
    </div>
  );
  if (!tip) return body;
  return (
    <Tooltip>
      <TooltipTrigger asChild>{body}</TooltipTrigger>
      <TooltipContent side="bottom">{tip}</TooltipContent>
    </Tooltip>
  );
}

const swap = {
  initial: { opacity: 0, y: 6, filter: "blur(4px)" },
  animate: { opacity: 1, y: 0, filter: "blur(0px)" },
  exit: { opacity: 0, y: -4, filter: "blur(4px)" },
  transition: { duration: 0.2, ease: easeOut },
};

/**
 * What this session has done so far, right under the power switch. While off it
 * shows the previous session's totals (or a one-line hint).
 */
export function SessionStats({
  visual,
  mode,
  live,
  now,
  last,
  idle,
  animated,
}: {
  visual: Visual;
  mode: Mode;
  live: Status | null;
  now: number;
  last: LastSession | null;
  idle: string;
  animated: boolean;
}) {
  const running = (visual === "on" || visual === "reconnecting") && !!live;
  const kind = running ? "live" : visual === "error" ? "error" : visual === "off" && last?.mode === mode ? "last" : visual === "off" ? "idle" : "busy";
  const since = live?.connected_since ?? null;
  const elapsed = running && since ? now - since : 0;
  const total = (live?.traffic.up_total ?? 0) + (live?.traffic.down_total ?? 0);
  const b = byteParts(total);
  const shields = live?.protections ?? [];
  const activeShields = shields.filter((p) => p.active);

  return (
    <div className="grid h-[58px] w-full place-items-center">
      <AnimatePresence mode="wait" initial={false}>
        {kind === "live" ? (
          <motion.div key={`live-${mode}`} {...swap} className="glass flex items-stretch divide-x divide-border rounded-[20px] py-2.5">
            {mode === "shadow" ? (
              <>
                <Stat label="Secured" tip={`↑ ${bytes(live!.traffic.up_total)} · ↓ ${bytes(live!.traffic.down_total)}`}>
                  <NumberFlow value={b.value} format={{ minimumFractionDigits: b.digits, maximumFractionDigits: b.digits }} suffix={` ${b.unit}`} animated={animated} {...flow} />
                </Stat>
                <Stat label="Session">
                  <Clock ms={elapsed} animated={animated} />
                </Stat>
                <Stat label="Latency">{live!.latency_ms != null ? <NumberFlow value={live!.latency_ms} suffix=" ms" animated={animated} {...flow} /> : "—"}</Stat>
              </>
            ) : (
              <>
                <Stat label="Session">
                  <Clock ms={elapsed} animated={animated} />
                </Stat>
                <Stat label="Protections" tip={activeShields.map((p) => p.label).join(" · ") || undefined}>
                  <NumberFlow value={activeShields.length} suffix={` of ${shields.length}`} animated={animated} {...flow} />
                </Stat>
              </>
            )}
          </motion.div>
        ) : kind === "busy" ? (
          <motion.span key="busy" {...swap} />
        ) : (
          <motion.p
            key={`${kind}-${mode}`}
            {...swap}
            className={cn("m-0 max-w-[290px] text-center text-[12.5px] leading-snug text-balance", kind === "error" ? "text-destructive" : "text-muted-foreground")}
          >
            {kind === "error"
              ? live?.error?.message ?? "Something went wrong"
              : kind === "last"
                ? mode === "shadow"
                  ? `Last session · ${bytes(last!.bytes)} secured · ${duration(last!.ms)}`
                  : `Last session · ${duration(last!.ms)} protected`
                : idle}
          </motion.p>
        )}
      </AnimatePresence>
    </div>
  );
}
