export function Logo({ compact = false }: { compact?: boolean }) {
  return <div className="brand" aria-label="Snapline">
    <span className="brand-mark"><i /><i /><i /></span>
    {!compact && <span>Snapline</span>}
  </div>;
}
