import type { ReactNode } from "react";
import { Icon } from "./Icon";

export function Toggle({ checked, onChange, disabled, label }: { checked: boolean; onChange: (v: boolean) => void; disabled?: boolean; label: string }) {
  return <button type="button" className="toggle" role="switch" aria-checked={checked} aria-label={label} disabled={disabled} onClick={() => onChange(!checked)} />;
}

export function Seg<T extends string>({ value, options, onChange, label }: { value: T; options: { value: T; label: string; disabled?: boolean }[]; onChange: (v: T) => void; label: string }) {
  return (
    <div className="seg" role="group" aria-label={label}>
      {options.map((o) => (
        <button key={o.value} type="button" aria-pressed={o.value === value} disabled={o.disabled} onClick={() => onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function SettingRow({ icon, label, sub, children }: { icon?: string; label: string; sub?: ReactNode; children?: ReactNode }) {
  return (
    <div className="row">
      {icon && (
        <span className="chip">
          <Icon name={icon} />
        </span>
      )}
      <span className="text">
        <span className="value" style={{ fontWeight: 540 }}>
          {label}
        </span>
        {sub && <span className="sub" style={{ whiteSpace: "normal" }}>{sub}</span>}
      </span>
      {children && <span className="trail">{children}</span>}
    </div>
  );
}

export function ToggleRow({ icon, label, sub, checked, onChange, disabled }: { icon?: string; label: string; sub?: ReactNode; checked: boolean; onChange: (v: boolean) => void; disabled?: boolean }) {
  return (
    <SettingRow icon={icon} label={label} sub={sub}>
      <Toggle checked={checked} onChange={onChange} disabled={disabled} label={label} />
    </SettingRow>
  );
}

export function NavRow({ icon, label, sub, onClick, trail }: { icon: string; label: string; sub?: ReactNode; onClick: () => void; trail?: ReactNode }) {
  return (
    <button type="button" className="row" onClick={onClick}>
      <span className="chip">
        <Icon name={icon} />
      </span>
      <span className="text">
        <span className="value" style={{ fontWeight: 540 }}>
          {label}
        </span>
        {sub && <span className="sub">{sub}</span>}
      </span>
      <span className="trail">
        {trail}
        <Icon name="chevron" />
      </span>
    </button>
  );
}

export function RadioRow({ label, sub, checked, onSelect, disabled }: { label: string; sub?: ReactNode; checked: boolean; onSelect: () => void; disabled?: boolean }) {
  return (
    <button type="button" className="row radio-row" role="radio" aria-checked={checked} onClick={onSelect} disabled={disabled}>
      <span className="mark" />
      <span className="text">
        <span className="value" style={{ fontWeight: 540 }}>
          {label}
        </span>
        {sub && <span className="sub" style={{ whiteSpace: "normal" }}>{sub}</span>}
      </span>
    </button>
  );
}

export function Page({ title, onBack, children, actions }: { title: string; onBack: () => void; children: ReactNode; actions?: ReactNode }) {
  return (
    <div className="page page-overlay">
      <div className="page-head">
        <button className="icon-btn" onClick={onBack} aria-label="Back">
          <Icon name="back" />
        </button>
        <h1>{title}</h1>
        <span className="spacer" />
        {actions}
      </div>
      <div className="page-body">{children}</div>
    </div>
  );
}
