import type { MouseEvent, ReactNode } from "react";

export function IconButton({
  active = false,
  danger = false,
  disabled = false,
  label,
  onClick,
  variant = "default",
  children,
}: {
  active?: boolean;
  danger?: boolean;
  disabled?: boolean;
  label: string;
  onClick: (event: MouseEvent<HTMLButtonElement>) => void;
  variant?: "default" | "floating";
  children: ReactNode;
}) {
  const className = [
    "iconButton",
    active ? "iconButtonActive" : "",
    danger ? "danger" : "",
    variant === "floating" ? "floatingIconButton" : "",
  ].filter(Boolean).join(" ");

  return (
    <button aria-label={label} className={className} disabled={disabled} onClick={(event) => {
      event.stopPropagation();
      onClick(event);
    }} title={label} type="button">
      {children}
    </button>
  );
}

export function LogoIcon() {
  return (
    <svg className="logoMark" viewBox="0 0 32 32" aria-hidden="true">
      <path d="M9 5.5h11l5 5v16H9z" />
      <path d="M20 5.5v5h5" />
      <path d="M13 15h8M11 20h10M13 24h6" />
      <path d="M6 16h3M5 21h4" />
    </svg>
  );
}

export function PlusIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg>;
}

export function SettingsIcon() {
  return (
    <svg className="gearIcon" viewBox="0 0 24 24" aria-hidden="true">
      <path d="M10.6 3.5h2.8l.45 2.15c.45.16.88.34 1.28.58l1.86-1.18 1.98 1.98-1.18 1.86c.24.4.42.83.58 1.28l2.15.45v2.8l-2.15.45c-.16.45-.34.88-.58 1.28l1.18 1.86-1.98 1.98-1.86-1.18c-.4.24-.83.42-1.28.58l-.45 2.15h-2.8l-.45-2.15a6.7 6.7 0 0 1-1.28-.58l-1.86 1.18-1.98-1.98 1.18-1.86a6.7 6.7 0 0 1-.58-1.28l-2.15-.45v-2.8l2.15-.45c.16-.45.34-.88.58-1.28L5.05 7.03l1.98-1.98 1.86 1.18c.4-.24.83-.42 1.28-.58l.43-2.15z" />
      <path d="M9.3 12a2.7 2.7 0 1 0 5.4 0 2.7 2.7 0 0 0-5.4 0z" />
    </svg>
  );
}

export function ListIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 6.5h10" /><path d="M7 12h10" /><path d="M7 17.5h10" /><path d="M4 6.5h.1M4 12h.1M4 17.5h.1" /></svg>;
}

export function PinIcon() {
  return <svg className="pinIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M9.2 4.5h5.6" /><path d="M10 4.5v5.1L6.9 14h10.2L14 9.6V4.5" /><path d="M12 14v6" /></svg>;
}

export function CheckIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12.5l4.2 4.2L19 6.8" /></svg>;
}

export function TrashIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 7h14M9 7V5h6v2M8 10v8M12 10v8M16 10v8" /><path d="M7 7l1 14h8l1-14" /></svg>;
}

export function CloseIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18" /></svg>;
}

export function MoreIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12h.1M12 12h.1M19 12h.1" /></svg>;
}

export function SourceModeIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 7l-4 5 4 5" /><path d="M15 7l4 5-4 5" /><path d="M13 5l-2 14" /></svg>;
}

export function PreviewModeIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M3.5 12s3.5-6.5 8.5-6.5 8.5 6.5 8.5 6.5-3.5 6.5-8.5 6.5S3.5 12 3.5 12Z" /><path d="M12 9.2a2.8 2.8 0 1 0 0 5.6 2.8 2.8 0 0 0 0-5.6Z" /></svg>;
}

export function ToolbarIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M4 7h2M9 7h2M14 7h2M4 12h16M4 17h8" /><rect x="3" y="5" width="5" height="4" rx="1.2"/></svg>;
}

export function ExportIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3v12M8 11l4 4 4-4"/><path d="M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2"/></svg>;
}

export function ConflictIcon() {
  return <svg className="conflictIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 9v4M12 17h.01"/><path d="M10.3 4l-8 13.5A2 2 0 0 0 4 20h16a2 2 0 0 0 1.7-2.5L13.7 4a2 2 0 0 0-3.4 0z"/></svg>;
}

export function StarIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" /></svg>;
}

export function ThemeSystemIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="3" width="18" height="18" rx="3" /><path d="M3 12h9v9H6a3 3 0 0 1-3-3v-6z" /></svg>;
}

export function ThemeLightIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="4" /><path d="M12 2v2M12 20v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M2 12h2M20 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42" /></svg>;
}

export function ThemeDarkIcon() {
  return <svg className="flatIcon" viewBox="0 0 24 24" aria-hidden="true"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" /></svg>;
}

export function ChevronDownIcon({ className = "flatIcon" }: { className?: string }) {
  return <svg className={className} viewBox="0 0 24 24" aria-hidden="true"><path d="m6 9 6 6 6-6" /></svg>;
}
