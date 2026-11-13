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

// Digit rolls settle well inside the one-second tick.
const flow = {
  transformTiming: { duration: 300, easing: "cubic-bezier(0.22, 1, 0.36, 1)" },
  spinTiming: { duration: 300, easing: "cubic-bezier(0.22, 1, 0.36, 1)" },
  opacityTiming: { duration: 160, easing: "ease-out" },
} as const;

function Clock({ ms, animated }: { ms: number; animated: boolean }) {
  const { h, m, s } = clockParts(ms);
  const two = { minimumIntegerDigits: 2 } as const;
  return (
    <NumberFlowGroup>
      <span className="inline-flex items-baseline">
        {h > 0 && (
          <>
            <NumberFlow value={h} trend={1} animated={animated} {...flow} />:
          </>
        )}
        <NumberFlow value={m} format={two} digits={{ 1: { max: 5 } }} trend={1} animated={animated} {...flow} />:
        <NumberFlow value={s} format={two} digits={{ 1: { max: 5 } }} trend={1} animated={animated} {...flow} />
      </span>
    </NumberFlowGroup>
  );
}

/** A figure in the foreground colour followed by its plain-language unit. */
function Fig({ children, unit }: { children: ReactNode; unit?: string }) {
  return (
    <span className="whitespace-nowrap">
      <span className="font-medium text-foreground tabular-nums">{children}</span>
      {unit && <span> {unit}</span>}
    </span>
  );
}

const Dot = () => <span className="text-faint">·</span>;

const swap = {
  initial: { opacity: 0, y: 4 },
  animate: { opacity: 1, y: 0 },
  exit: { opacity: 0, y: -3 },
  transition: { duration: 0.14, ease: easeOut },
};

/**
 * One quiet line under the switch: what this session has done so far. While
 * off it shows the previous session's totals, or a short hint.
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
  const active = (live?.protections ?? []).filter((p) => p.active);

  return (
    <div className="grid h-10 w-full place-items-center">
      <AnimatePresence mode="wait" initial={false}>
        {kind === "live" ? (
          <motion.div key={`live-${mode}`} {...swap} className="flex items-baseline gap-2 text-[13.5px] text-muted-foreground">
            {mode === "shadow" ? (
              <>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <span tabIndex={0} className="outline-none">
                      <Fig unit="secured">
                        <NumberFlow value={b.value} format={{ minimumFractionDigits: b.digits, maximumFractionDigits: b.digits }} suffix={` ${b.unit}`} animated={animated} {...flow} />
                      </Fig>
                    </span>
                  </TooltipTrigger>
                  <TooltipContent side="bottom">{`${bytes(live!.traffic.up_total)} up, ${bytes(live!.traffic.down_total)} down`}</TooltipContent>
                </Tooltip>
                <Dot />
                <Fig>
                  <Clock ms={elapsed} animated={animated} />
                </Fig>
                {live!.latency_ms != null && (
                  <>
                    <Dot />
                    <Fig unit="ms">
                      <NumberFlow value={live!.latency_ms} animated={animated} {...flow} />
                    </Fig>
                  </>
                )}
              </>
            ) : (
              <>
                <Fig>
                  <Clock ms={elapsed} animated={animated} />
                </Fig>
                <Dot />
                <Fig unit={active.length === 1 ? "protection on" : "protections on"}>{active.length}</Fig>
              </>
            )}
          </motion.div>
        ) : kind === "busy" ? (
          <motion.span key="busy" {...swap} />
        ) : (
          <motion.p
            key={`${kind}-${mode}`}
            {...swap}
            className={cn("m-0 max-w-[300px] text-center text-[12.5px] leading-snug text-balance", kind === "error" ? "text-destructive" : "text-muted-foreground")}
          >
            {kind === "error"
              ? live?.error?.message ?? "Something went wrong"
              : kind === "last"
                ? mode === "shadow"
                  ? `Last session: ${bytes(last!.bytes)} in ${duration(last!.ms)}`
                  : `Last session: ${duration(last!.ms)}`
                : idle}
          </motion.p>
        )}
      </AnimatePresence>
    </div>
  );
}
