import { useState, type ReactNode } from "react";
import { ChevronLeft, ChevronRight, type LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";
import { startWindowDrag } from "@/lib/window";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";

/* ---------- Layout ------------------------------------------------------ */

export function Page({ title, onBack, children, actions }: { title: string; onBack: () => void; children: ReactNode; actions?: ReactNode }) {
  return (
    <div className="flex h-full flex-col">
      <div className="flex h-11 shrink-0 items-center gap-1 pr-3 pl-2" onMouseDown={startWindowDrag}>
        <IconButton label="Back" onClick={onBack}>
          <ChevronLeft className="size-[18px]" />
        </IconButton>
        <h1 className="m-0 text-[17px] font-semibold tracking-[-0.015em]">{title}</h1>
        <span className="flex-1" />
        {actions}
      </div>
      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-4 pt-1 pb-6 *:shrink-0">{children}</div>
    </div>
  );
}

/** A grouped glass list, optionally titled. */
export function List({ title, hint, children, className }: { title?: string; hint?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <section className="flex flex-col">
      {title && <SectionTitle>{title}</SectionTitle>}
      <Card className={cn("p-1", className)}>{children}</Card>
      {hint && <Hint className="mt-2">{hint}</Hint>}
    </section>
  );
}

export function SectionTitle({ children }: { children: ReactNode }) {
  return <h2 className="m-0 px-3 pt-1 pb-1.5 text-[12.5px] font-medium text-muted-foreground">{children}</h2>;
}

export function Hint({ children, className }: { children: ReactNode; className?: string }) {
  return <p className={cn("m-0 px-3 text-[12px] leading-relaxed text-faint", className)}>{children}</p>;
}

/* ---------- Rows -------------------------------------------------------- */

const rowBase =
  "relative flex w-full min-h-[46px] items-center gap-3 rounded-[16px] px-3 py-2 text-left not-first:before:absolute not-first:before:top-0 not-first:before:right-3 not-first:before:left-3 not-first:before:h-px not-first:before:bg-border";

function RowIcon({ icon: I }: { icon?: LucideIcon }) {
  return I ? <I className="size-[17px] shrink-0 text-muted-foreground" strokeWidth={1.8} aria-hidden="true" /> : null;
}

function RowText({ label, sub, mono }: { label: ReactNode; sub?: ReactNode; mono?: boolean }) {
  return (
    <span className="flex min-w-0 flex-1 flex-col gap-px">
      <span className={cn("truncate text-[13.5px] font-medium", mono && "font-mono text-[12.5px] tabular-nums")}>{label}</span>
      {sub && <span className="text-[12px] leading-snug text-muted-foreground">{sub}</span>}
    </span>
  );
}

export function SettingRow({ icon, label, sub, mono, children }: { icon?: LucideIcon; label: ReactNode; sub?: ReactNode; mono?: boolean; children?: ReactNode }) {
  return (
    <div className={rowBase}>
      <RowIcon icon={icon} />
      <RowText label={label} sub={sub} mono={mono} />
      {children && <span className="flex min-w-0 shrink-0 items-center gap-2 text-[12.5px] text-muted-foreground">{children}</span>}
    </div>
  );
}

export function NavRow({ icon, label, sub, value, onClick }: { icon?: LucideIcon; label: ReactNode; sub?: ReactNode; value?: ReactNode; onClick: () => void }) {
  return (
    <button type="button" data-spot className={cn(rowBase, "transition-colors active:bg-fill")} onClick={onClick}>
      <RowIcon icon={icon} />
      <RowText label={label} sub={sub} />
      <span className="flex min-w-0 shrink items-center gap-1 text-[12.5px] text-muted-foreground">
        {value != null && <span className="truncate">{value}</span>}
        <ChevronRight className="size-4 shrink-0 text-faint" />
      </span>
    </button>
  );
}

export function ToggleRow({ icon, label, sub, checked, onChange, disabled }: { icon?: LucideIcon; label: string; sub?: ReactNode; checked: boolean; onChange: (v: boolean) => void; disabled?: boolean }) {
  return (
    <label data-spot className={cn(rowBase, disabled ? "cursor-default" : "cursor-pointer")}>
      <RowIcon icon={icon} />
      <RowText label={label} sub={sub} />
      <Switch checked={checked} onCheckedChange={onChange} disabled={disabled} aria-label={label} />
    </label>
  );
}

export function RadioList<T extends string>({ value, onChange, options, label }: { value: T; onChange: (v: T) => void; options: { value: T; label: string; sub?: ReactNode; disabled?: boolean }[]; label: string }) {
  return (
    <RadioGroup value={value} onValueChange={(v) => onChange(v as T)} aria-label={label} className="gap-0">
      {options.map((o) => (
        <label key={o.value} data-spot className={cn(rowBase, "cursor-pointer")}>
          <RadioGroupItem value={o.value} disabled={o.disabled} className="size-[18px]" />
          <RowText label={o.label} sub={o.sub} />
        </label>
      ))}
    </RadioGroup>
  );
}

/* ---------- Controls ---------------------------------------------------- */

/** Segmented control. Equal segments, so the selection pill slides with a plain transform. */
export function Seg<T extends string>({ value, options, onChange, label, className }: { value: T; options: { value: T; label: string; disabled?: boolean }[]; onChange: (v: T) => void; label: string; className?: string }) {
  const index = Math.max(0, options.findIndex((o) => o.value === value));
  return (
    <ToggleGroup
      type="single"
      spacing={0.5}
      value={value}
      onValueChange={(v) => v && onChange(v as T)}
      aria-label={label}
      className={cn("relative grid auto-cols-fr grid-flow-col !gap-0 rounded-full bg-fill p-[3px]", className)}
    >
      <span
        aria-hidden="true"
        className="absolute top-[3px] bottom-[3px] left-[3px] rounded-full bg-white shadow-[0_1px_3px_rgb(0_0_0/0.1)] transition-transform duration-200 ease-out dark:bg-white/14"
        style={{ width: `calc((100% - 6px) / ${options.length})`, transform: `translateX(${index * 100}%)` }}
      />
      {options.map((o) => (
        <ToggleGroupItem
          key={o.value}
          value={o.value}
          disabled={o.disabled}
          className="relative h-7 rounded-full px-3 text-[12px] font-medium text-foreground/70 transition-colors duration-150 hover:bg-transparent hover:text-foreground data-[state=on]:bg-transparent data-[state=on]:text-foreground"
        >
          {o.label}
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  );
}

export function IconButton({ label, children, onClick, className, disabled, variant = "ghost" }: { label: string; children: ReactNode; onClick?: () => void; className?: string; disabled?: boolean; variant?: "ghost" | "secondary" | "destructive" }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant={variant} size="icon-sm" aria-label={label} onClick={onClick} disabled={disabled} className={className}>
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

/** A collapsed "Advanced" section. */
export function Advanced({ children, label = "Advanced" }: { children: ReactNode; label?: string }) {
  const [open, setOpen] = useState(false);
  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger asChild>
        <button type="button" data-spot className="flex items-center gap-1.5 rounded-full px-3 py-1.5 text-[12.5px] font-medium text-muted-foreground transition-colors hover:text-foreground">
          <ChevronRight className={cn("size-3.5 transition-transform duration-200 ease-out", open && "rotate-90")} />
          {label}
        </button>
      </CollapsibleTrigger>
      <CollapsibleContent className="overflow-hidden data-open:animate-collapsible-down data-closed:animate-collapsible-up">
        <div className="flex flex-col gap-3 pt-2">{children}</div>
      </CollapsibleContent>
    </Collapsible>
  );
}

/** Tinted glass notice for errors and warnings. */
export function Notice({ icon: I, tone = "bad", children }: { icon: LucideIcon; tone?: "bad" | "info"; children: ReactNode }) {
  return (
    <div
      className={cn(
        "glass flex items-start gap-3 rounded-[20px] p-3.5 text-[12.5px]",
        tone === "bad" ? "bg-[color-mix(in_srgb,var(--destructive)_9%,var(--glass))]" : "bg-[color-mix(in_srgb,var(--primary)_8%,var(--glass))]",
      )}
    >
      <I className={cn("mt-px size-[18px] shrink-0", tone === "bad" ? "text-destructive" : "text-primary")} />
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  );
}

/** Small status capsule. */
export function Pill({ tone = "neutral", children, className }: { tone?: "neutral" | "good" | "bad" | "warn" | "accent"; children: ReactNode; className?: string }) {
  const tones = {
    neutral: "bg-fill text-muted-foreground",
    good: "bg-good/14 text-good",
    bad: "bg-destructive/12 text-destructive",
    warn: "bg-warn/15 text-warn",
    accent: "bg-primary/12 text-primary",
  } as const;
  return <span className={cn("inline-flex h-6 items-center gap-1.5 rounded-full px-2.5 text-[11.5px] font-medium whitespace-nowrap [&_svg]:size-3", tones[tone], className)}>{children}</span>;
}
