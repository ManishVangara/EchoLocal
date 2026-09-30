// Basic form controls styled like macOS System Settings.

import type { ReactNode } from "react";

export function Switch(props: { checked: boolean; onChange: (value: boolean) => void; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={props.checked}
      aria-label={props.label}
      className={`switch ${props.checked ? "on" : ""}`}
      onClick={() => props.onChange(!props.checked)}
    >
      <span className="knob" />
    </button>
  );
}

export function Segmented<T extends string>(props: {
  value: T;
  options: [T, string][];
  onChange: (value: T) => void;
  label: string;
}) {
  return (
    <div className="segmented" role="radiogroup" aria-label={props.label}>
      {props.options.map(([value, label]) => (
        <button
          key={value}
          type="button"
          role="radio"
          aria-checked={props.value === value}
          className={props.value === value ? "active" : ""}
          onClick={() => props.onChange(value)}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

/** A labelled row inside a group card: label (and detail) left, control right. */
export function Row(props: { label: ReactNode; detail?: ReactNode; children?: ReactNode }) {
  return (
    <div className="row">
      <div className="row-text">
        <div className="row-label">{props.label}</div>
        {props.detail && <div className="row-detail">{props.detail}</div>}
      </div>
      {props.children && <div className="row-control">{props.children}</div>}
    </div>
  );
}

export function Group(props: { title?: string; footer?: ReactNode; children: ReactNode }) {
  return (
    <section className="group">
      {props.title && <h3 className="group-title">{props.title}</h3>}
      <div className="group-card">{props.children}</div>
      {props.footer && <p className="group-footer">{props.footer}</p>}
    </section>
  );
}

export function PageHeader(props: { title: string; subtitle?: string }) {
  return (
    <header className="page-header">
      <h1>{props.title}</h1>
      {props.subtitle && <p>{props.subtitle}</p>}
    </header>
  );
}

/** "Right ⌥ Space" → ["Right ⌥", "Space"]: side prefixes stay with their key. */
export function keycapParts(label: string): string[] {
  const parts: string[] = [];
  for (const token of label.split(" ").filter(Boolean)) {
    const previous = parts[parts.length - 1];
    if (previous === "Left" || previous === "Right") {
      parts[parts.length - 1] = `${previous} ${token}`;
    } else {
      parts.push(token);
    }
  }
  return parts;
}

export function Keycap({ label, large }: { label: string; large?: boolean }) {
  return (
    <span className={`keycaps ${large ? "large" : ""}`}>
      {keycapParts(label).map((part, i) => (
        <kbd key={i}>{part}</kbd>
      ))}
    </span>
  );
}
