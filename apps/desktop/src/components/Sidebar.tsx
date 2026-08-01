import { Archive, Bot, CircleDot, Hash, Inbox, LogOut, Pin, Plus, Settings, Sparkles, Tag } from 'lucide-react';
import type { Item, View } from '../types';
import { Logo } from './Logo';

export function Sidebar({ items, view, onView, onNew, onAi, onSettings, onLogout }: { items: Item[]; view: View; onView: (view: View) => void; onNew: () => void; onAi: () => void; onSettings: () => void; onLogout: () => void }) {
  const tags = [...new Set(items.flatMap((item) => item.content.tags))].slice(0, 5);
  const markers = [...new Set(['账目', '重要', '待办', ...items.flatMap((item) => item.content.markers)])].slice(0, 5);
  return <aside className="sidebar">
    <div className="sidebar-head"><Logo /><button className="icon-button" aria-label="设置" title="设置" onClick={onSettings}><Settings size={17} /></button></div>
    <button className="new-record-button" onClick={onNew}><Plus size={17} />新记录<span>⌘ N</span></button>
    <nav className="main-nav">
      <button className={view === 'all' ? 'active' : ''} onClick={() => onView('all')}><Inbox size={17} />全部记录<em>{items.filter((i) => !i.archived).length}</em></button>
      <button className={view === 'pinned' ? 'active' : ''} onClick={() => onView('pinned')}><Pin size={17} />收藏</button>
      <button className={view === 'archive' ? 'active' : ''} onClick={() => onView('archive')}><Archive size={17} />归档</button>
      <button onClick={onAi}><Bot size={17} />AI 对话<span className="new-pill"><Sparkles size={11} />AI</span></button>
    </nav>
    <div className="nav-section"><div className="nav-label"><span><CircleDot size={13} />特殊标记</span></div>{markers.map((marker, index) => <button key={marker} className={view === `marker:${marker}` ? 'active' : ''} onClick={() => onView(`marker:${marker}`)}><span className={`marker-dot tone-${index % 4}`} />{marker}</button>)}</div>
    <div className="nav-section"><div className="nav-label"><span><Tag size={13} />标签</span></div>{tags.map((tag) => <button key={tag} className={view === `tag:${tag}` ? 'active' : ''} onClick={() => onView(`tag:${tag}`)}><Hash size={14} />{tag}</button>)}</div>
    <div className="sidebar-user"><span>W</span><div><strong>wtlll</strong><small>已加密同步</small></div><button className="icon-button" title="退出登录" aria-label="退出登录" onClick={onLogout}><LogOut size={16} /></button></div>
  </aside>;
}
