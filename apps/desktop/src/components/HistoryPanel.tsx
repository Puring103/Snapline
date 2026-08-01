import { Archive, AudioLines, Camera, ChevronRight, FileText, Image, Search, Video, X } from 'lucide-react';
import { useMemo, useState } from 'react';
import { excerpt } from '../lib/format';
import type { Item } from '../types';

const sourceIcons = { text: FileText, screenshot: Camera, image: Image, audio: AudioLines, video: Video, mixed: FileText };
const PAGE_SIZE = 50;

export function HistoryPanel({ items, onClose, onSelect }: { items: Item[]; onClose: () => void; onSelect: (id: string) => void }) {
  const [query, setQuery] = useState('');
  const [visible, setVisible] = useState(PAGE_SIZE);
  const filtered = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    return [...items]
      .sort((left, right) => right.updated_at.localeCompare(left.updated_at))
      .filter((item) => !normalized || [item.content.title, item.content.markdown, ...item.content.tags, ...item.content.markers].join(' ').toLocaleLowerCase().includes(normalized));
  }, [items, query]);

  return <div className="history-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
    <aside className="history-panel" role="dialog" aria-modal="true" aria-label="历史记录">
      <header><div><strong>历史记录</strong><span>{filtered.length} 条</span></div><button className="icon-button" aria-label="关闭历史记录" onClick={onClose}><X size={17} /></button></header>
      <label className="history-search"><Search size={15} /><input value={query} onChange={(event) => { setQuery(event.target.value); setVisible(PAGE_SIZE); }} placeholder="搜索历史记录" /></label>
      <div className="history-list">
        {filtered.length === 0 && <div className="empty-state"><Search size={21} /><strong>没有找到记录</strong></div>}
        {filtered.slice(0, visible).map((item) => {
          const Icon = sourceIcons[item.content.source_type];
          return <button className="history-row" key={item.id} onClick={() => { onSelect(item.id); onClose(); }}>
            <span className="history-type"><Icon size={15} /></span>
            <span className="history-copy"><strong>{item.content.title || '无标题记录'}</strong><small>{excerpt(item.content.markdown) || '无正文内容'}</small><em>{[...item.content.markers, ...item.content.tags.map((tag) => `#${tag}`)].slice(0, 3).join(' · ')}</em></span>
            <span className="history-meta">{item.archived && <i><Archive size={11} />已归档</i>}<time>{new Date(item.updated_at).toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', hour12: false })}</time></span>
            <ChevronRight size={15} />
          </button>;
        })}
        {visible < filtered.length && <button className="history-more" onClick={() => setVisible((value) => value + PAGE_SIZE)}>加载更多</button>}
      </div>
    </aside>
  </div>;
}
