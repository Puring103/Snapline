export function PlusIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg>;
}

export function SearchIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M10.8 18.1a7.3 7.3 0 1 1 0-14.6 7.3 7.3 0 0 1 0 14.6Z" /><path d="M16.2 16.2 21 21" /></svg>;
}

export function PinIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9.2 4.5h5.6" /><path d="M10 4.5v5.1L6.9 14h10.2L14 9.6V4.5" /><path d="M12 14v6" /></svg>;
}

export function TrashIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 7h14M9 7V5h6v2M8 10v8M12 10v8M16 10v8" /><path d="M7 7l1 14h8l1-14" /></svg>;
}

export function BackIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M15 6 9 12l6 6" /></svg>;
}

export function MenuIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 7h16M4 12h16M4 17h16" /></svg>;
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

export function SettingsIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M10.6 3.5h2.8l.45 2.15c.45.16.88.34 1.28.58l1.86-1.18 1.98 1.98-1.18 1.86c.24.4.42.83.58 1.28l2.15.45v2.8l-2.15.45c-.16.45-.34.88-.58 1.28l1.18 1.86-1.98 1.98-1.86-1.18c-.4.24-.83.42-1.28.58l-.45 2.15h-2.8l-.45-2.15a6.7 6.7 0 0 1-1.28-.58l-1.86 1.18-1.98-1.98 1.18-1.86a6.7 6.7 0 0 1-.58-1.28l-2.15-.45v-2.8l2.15-.45c.16-.45.34-.88.58-1.28L5.05 7.03l1.98-1.98 1.86 1.18c.4-.24.83-.42 1.28-.58l.43-2.15z" /><path d="M9.3 12a2.7 2.7 0 1 0 5.4 0 2.7 2.7 0 0 0-5.4 0z" /></svg>;
}

export function PreviewIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3.5 12s3.5-6.5 8.5-6.5 8.5 6.5 8.5 6.5-3.5 6.5-8.5 6.5S3.5 12 3.5 12Z" /><path d="M12 9.2a2.8 2.8 0 1 0 0 5.6 2.8 2.8 0 0 0 0-5.6Z" /></svg>;
}

export function WriteIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 20h4l11-11a2.8 2.8 0 0 0-4-4L4 16v4Z" /><path d="M13.5 6.5l4 4" /></svg>;
}

export function SyncIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M20 7v5h-5" /><path d="M4 17v-5h5" /><path d="M18.2 9A7 7 0 0 0 6 6.8L4 9" /><path d="M5.8 15A7 7 0 0 0 18 17.2l2-2.2" /></svg>;
}

export function BoldIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5h5a3.5 3.5 0 0 1 0 7H8z" /><path d="M8 12h6a3.5 3.5 0 0 1 0 7H8z" /></svg>;
}

export function HeadingIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 5v14M19 5v14M5 12h14M12 19h7" /></svg>;
}

export function ListIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 6h12M8 12h12M8 18h12" /><path d="M4 6h.1M4 12h.1M4 18h.1" /></svg>;
}

export function IconButton({
  label,
  active,
  danger,
  onClick,
  children,
}: {
  label: string;
  active?: boolean;
  danger?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      aria-label={label}
      className={["iconButton", active ? "active" : "", danger ? "danger" : ""].filter(Boolean).join(" ")}
      onClick={onClick}
      title={label}
      type="button"
    >
      {children}
    </button>
  );
}
