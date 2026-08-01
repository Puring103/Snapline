import { useCallback, useEffect, useMemo, useState } from 'react';
import { Bot, Camera, History, Mic, Plus, RotateCw, Search, Sparkles } from 'lucide-react';
import { AiPanel } from './components/AiPanel';
import { Capture } from './components/Capture';
import { EditorPane } from './components/EditorPane';
import { Login } from './components/Login';
import { Sidebar } from './components/Sidebar';
import { Timeline } from './components/Timeline';
import { deleteItem, emptyContent, listItems, saveItem } from './lib/repository';
import { logout, sessionStatus } from './lib/native';
import type { Item, SourceType, View } from './types';

function captureTypeFromUrl(): SourceType | null {
  const type = new URLSearchParams(location.search).get('capture');
  return ['text', 'screenshot', 'audio', 'image', 'video'].includes(type || '') ? type as SourceType : null;
}

export function App() {
  const [authenticated, setAuthenticated] = useState(false);
  const [sessionChecked, setSessionChecked] = useState(false);
  const [items, setItems] = useState<Item[]>([]);
  const [selectedId, setSelectedId] = useState<string>();
  const [view, setView] = useState<View>('all');
  const [query, setQuery] = useState('');
  const [captureType, setCaptureType] = useState<SourceType | null>(captureTypeFromUrl());
  const [aiOpen, setAiOpen] = useState(false);

  const reload = useCallback(async () => {
    const loaded = await listItems();
    setItems(loaded);
    setSelectedId((current) => current || loaded[0]?.id);
  }, []);
  useEffect(() => { void sessionStatus().then((status) => setAuthenticated(status.authenticated)).finally(() => setSessionChecked(true)); }, []);
  useEffect(() => { if (authenticated) void reload(); }, [authenticated, reload]);

  const filteredItems = useMemo(() => items.filter((item) => {
    if (view === 'pinned' && !item.pinned) return false;
    if (view === 'archive' ? !item.archived : item.archived) return false;
    if (view.startsWith('marker:') && !item.content.markers.includes(view.slice(7))) return false;
    if (view.startsWith('tag:') && !item.content.tags.includes(view.slice(4))) return false;
    const search = query.trim().toLocaleLowerCase();
    if (!search) return true;
    return [item.content.title, item.content.markdown, ...item.content.tags, ...item.content.markers, item.content.ai_metadata?.search_text || ''].join(' ').toLocaleLowerCase().includes(search);
  }), [items, query, view]);
  const selected = items.find((item) => item.id === selectedId);

  const changeItem = useCallback((changed: Item) => setItems((current) => current.map((item) => item.id === changed.id ? changed : item)), []);
  const persistItem = useCallback(async (changed: Item) => {
    const saved = await saveItem(changed);
    setItems((current) => current.map((item) => item.id === saved.id ? saved : item));
  }, []);
  const create = (type: SourceType = 'text') => {
    const now = new Date().toISOString();
    const item: Item = { id: crypto.randomUUID(), content: emptyContent(type), created_at: now, updated_at: now, version: 0, archived: false, pinned: false, sync_status: 'pending' };
    setItems((current) => [item, ...current]); setSelectedId(item.id);
  };
  const remove = async (id: string) => { await deleteItem(id); const remaining = items.filter((item) => item.id !== id); setItems(remaining); setSelectedId(remaining[0]?.id); };

  if (!sessionChecked) return <main className="session-loading"><Sparkles size={24} /><span>正在检查加密会话…</span></main>;
  if (!authenticated) return <Login onAuthenticated={() => setAuthenticated(true)} />;
  if (captureType) return <Capture initialType={captureType} onClose={() => { setCaptureType(null); history.replaceState(null, '', location.pathname); }} onCreated={(item) => setItems((current) => [item, ...current.filter((value) => value.id !== item.id)])} />;

  return <main className={`app-shell ${aiOpen ? 'with-ai' : ''}`}>
    <Sidebar items={items} view={view} onView={setView} onNew={() => create()} onAi={() => setAiOpen(true)} onLogout={() => { void logout().finally(() => { setItems([]); setAuthenticated(false); }); }} />
    <section className="workspace">
      <header className="workspace-toolbar"><div className="quick-actions"><button className="primary-button" onClick={() => create()}><Plus size={16} />文本</button><button className="tool-button" onClick={() => setCaptureType('screenshot')}><Camera size={16} />截图</button><button className="tool-button" onClick={() => setCaptureType('audio')}><Mic size={16} />录音</button></div><div className="workspace-actions"><span className="sync-indicator"><RotateCw size={13} />已加密同步</span><button className="tool-button"><History size={16} />历史记录</button><button className="icon-button" title="全局搜索" aria-label="全局搜索"><Search size={17} /></button><button className="icon-button ai-toggle" title="AI 对话" aria-label="AI 对话" onClick={() => setAiOpen(!aiOpen)}><Bot size={17} /></button></div></header>
      <div className="workspace-grid"><Timeline items={filteredItems} selectedId={selectedId} query={query} onQuery={setQuery} onSelect={setSelectedId} /><EditorPane item={selected} onChange={changeItem} onSave={persistItem} onDelete={(id) => void remove(id)} /></div>
    </section>
    {aiOpen && <AiPanel configured={false} onClose={() => setAiOpen(false)} />}
  </main>;
}
