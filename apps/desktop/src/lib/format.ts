export function relativeDate(value: string): string {
  const date = new Date(value);
  const now = new Date('2026-08-01T05:00:00.000Z');
  const day = 24 * 60 * 60 * 1000;
  if (now.getTime() - date.getTime() < day && now.getDate() === date.getDate()) return `今天 ${date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', hour12: false })}`;
  if (now.getTime() - date.getTime() < day * 2) return `昨天 ${date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', hour12: false })}`;
  return date.toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' });
}

export function excerpt(markdown: string): string {
  return markdown.replace(/[#>*_`\[\]-]/g, ' ').replace(/\s+/g, ' ').trim().slice(0, 110);
}
