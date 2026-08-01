import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from 'react';
import { Bot, Camera, History, Mic, Plus, RotateCw, Search, Sparkles } from 'lucide-react';
import { AiPanel } from './components/AiPanel';
import { AiSettings } from './components/AiSettings';
import { Capture } from './components/Capture';
import { ConflictPanel } from './components/ConflictPanel';
import { Login } from './components/Login';
import { Sidebar } from './components/Sidebar';
import { Timeline } from './components/Timeline';
import { HistoryPanel } from './components/HistoryPanel';
import { deleteItem, emptyContent, listItems, saveItem } from './lib/repository';
import { getAiConfig, logout, processAiQueue, sessionStatus, syncNow, type AiConfigStatus } from './lib/native';
import type { Item, RecordFilters, SourceType, View } from './types';

const EditorPane = lazy(() => import('./components/EditorPane'));

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
  const [filters, setFilters] = useState<RecordFilters>({ sourceTypes: [], tags: [], markers: [] });
  const [captureType, setCaptureType] = useState<SourceType | null>(captureTypeFromUrl());
  const [aiOpen, setAiOpen] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [aiSettingsOpen, setAiSettingsOpen] = useState(false);
  const [conflictOpen, setConflictOpen] = useState(false);
  const [syncState, setSyncState] = useState<'idle' | 'syncing' | 'error'>('idle');
  const [syncError, setSyncError] = useState('');
  const [conflictCount, setConflictCount] = useState(0);
  const [aiConfig, setAiConfig] = useState<AiConfigStatus>({ configured: false, base_url: null, model: null, processing: false });

  const reload = useCallback(async () => {
    const loaded = await listItems();
    setItems(loaded);
    setSelectedId((current) => current || loaded[0]?.id);
  }, []);
  const runSync = useCallback(async () => {
    setSyncState('syncing'); setSyncError('');
    try {
      const result = await syncNow();
      setConflictCount(result.conflicts);
      if (result.pulled || result.conflicts) await reload();
      setSyncState('idle');
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      setSyncError(message); setSyncState('error');
      const status = await sessionStatus().catch(() => ({ authenticated: true, user_id: null }));
      if (!status.authenticated) { setItems([]); setAuthenticated(false); }
    }
  }, [reload]);
  useEffect(() => { void sessionStatus().then((status) => setAuthenticated(status.authenticated)).finally(() => setSessionChecked(true)); }, []);
  useEffect(() => { if (authenticated) void reload(); }, [authenticated, reload]);
  useEffect(() => { if (authenticated) void runSync(); }, [authenticated, runSync]);
  useEffect(() => {
    if (!authenticated) return;
    const interval = window.setInterval(() => void runSync(), 30_000);
    const online = () => void runSync();
    window.addEventListener('online', online);
    return () => { window.clearInterval(interval); window.removeEventListener('online', online); };
  }, [authenticated, runSync]);
  useEffect(() => {
    if (!authenticated) return;
    void getAiConfig().then((status) => {
      setAiConfig(status);
      if (status.configured) void processAiQueue().then((result) => { if (result.completed) void reload(); });
      else setItems((current) => current.map((item) => item.ai_status === 'complete' ? item : { ...item, ai_status: 'unconfigured' }));
    });
  }, [authenticated, reload]);

  const filteredItems = useMemo(() => items.filter((item) => {
    if (view === 'pinned' && !item.pinned) return false;
    if (view === 'archive' ? !item.archived : item.archived) return false;
    if (view.startsWith('marker:') && !item.content.markers.includes(view.slice(7))) return false;
    if (view.startsWith('tag:') && !item.content.tags.includes(view.slice(4))) return false;
    if (filters.sourceTypes.length && !filters.sourceTypes.includes(item.content.source_type)) return false;
    if (!filters.tags.every((tag) => item.content.tags.includes(tag))) return false;
    if (!filters.markers.every((marker) => item.content.markers.includes(marker))) return false;
    const search = query.trim().toLocaleLowerCase();
    if (!search) return true;
    return [item.content.title, item.content.markdown, ...item.content.tags, ...item.content.markers, item.content.ai_metadata?.search_text || ''].join(' ').toLocaleLowerCase().includes(search);
  }), [filters, items, query, view]);
  const selected = items.find((item) => item.id === selectedId);

  const changeItem = useCallback((changed: Item) => setItems((current) => current.map((item) => item.id === changed.id ? changed : item)), []);
  const persistItem = useCallback(async (changed: Item) => {
    const saved = await saveItem(changed);
    const visible = aiConfig.configured ? saved : { ...saved, ai_status: 'unconfigured' as const };
    setItems((current) => current.map((item) => item.id === saved.id ? visible : item));
    if (aiConfig.configured) void processAiQueue().then((result) => { if (result.completed) void reload(); });
    void runSync();
  }, [aiConfig.configured, reload, runSync]);
  const create = (type: SourceType = 'text') => {
    const now = new Date().toISOString();
    const item: Item = { id: crypto.randomUUID(), content: emptyContent(type), created_at: now, updated_at: now, version: 0, archived: false, pinned: false, sync_status: 'pending', ai_status: aiConfig.configured ? 'pending' : 'unconfigured' };
    setItems((current) => [item, ...current]); setSelectedId(item.id);
  };
  const remove = async (id: string) => { await deleteItem(id); const remaining = items.filter((item) => item.id !== id); setItems(remaining); setSelectedId(remaining[0]?.id); void runSync(); };
  const updateFlags = (item: Item, patch: Pick<Partial<Item>, 'archived' | 'pinned'>) => {
    const changed = { ...item, ...patch };
    changeItem(changed);
    void persistItem(changed);
  };

  if (!sessionChecked) return <main className="session-loading"><Sparkles size={24} /><span>正在检查加密会话…</span></main>;
  if (!authenticated) return <Login onAuthenticated={() => setAuthenticated(true)} />;
  if (captureType) return <Capture initialType={captureType} onClose={() => { setCaptureType(null); history.replaceState(null, '', location.pathname); }} onCreated={(item) => { setItems((current) => [{ ...item, ai_status: aiConfig.configured ? item.ai_status : 'unconfigured' }, ...current.filter((value) => value.id !== item.id)]); void runSync(); }} />;

  return <main className={`app-shell ${aiOpen ? 'with-ai' : ''}`}>
    <Sidebar items={items} view={view} onView={setView} onNew={() => create()} onAi={() => setAiOpen(true)} onSettings={() => setAiSettingsOpen(true)} onLogout={() => { void logout().finally(() => { setItems([]); setAuthenticated(false); }); }} />
    <section className="workspace">
      <header className="workspace-toolbar"><div className="quick-actions"><button className="primary-button" onClick={() => create()}><Plus size={16} />文本</button><button className="tool-button" onClick={() => setCaptureType('screenshot')}><Camera size={16} />截图</button><button className="tool-button" onClick={() => setCaptureType('audio')}><Mic size={16} />录音</button></div><div className="workspace-actions">{conflictCount ? <button className="sync-indicator conflict" onClick={() => setConflictOpen(true)}><RotateCw size={13} />{conflictCount} 个冲突</button> : <span className={`sync-indicator ${syncState}`} title={syncError}><RotateCw size={13} />{syncState === 'syncing' ? '正在加密同步' : syncState === 'error' ? '同步失败' : '已加密同步'}</span>}<button className="tool-button" onClick={() => setHistoryOpen(true)}><History size={16} />历史记录</button><button className="icon-button" title="全局搜索" aria-label="全局搜索"><Search size={17} /></button><button className="icon-button ai-toggle" title="AI 对话" aria-label="AI 对话" onClick={() => setAiOpen(!aiOpen)}><Bot size={17} /></button></div></header>
      <div className="workspace-grid"><Timeline items={filteredItems} allItems={items} filters={filters} onFilters={setFilters} selectedId={selectedId} query={query} onQuery={setQuery} onSelect={setSelectedId} onPin={(item) => updateFlags(item, { pinned: !item.pinned })} onArchive={(item) => updateFlags(item, { archived: !item.archived })} onDelete={(id) => void remove(id)} /><Suspense fallback={<section className="editor-pane empty-editor"><Sparkles size={24} /><span>正在打开 Markdown 编辑器…</span></section>}><EditorPane item={selected} onChange={changeItem} onSave={persistItem} onDelete={(id) => void remove(id)} onRecord={() => setCaptureType('audio')} /></Suspense></div>
    </section>
    {aiOpen && <AiPanel configured={aiConfig.configured} onConfigure={() => setAiSettingsOpen(true)} onClose={() => setAiOpen(false)} onSelectCitation={(id) => { setView('all'); setSelectedId(id); }} />}
    {historyOpen && <HistoryPanel items={items} onClose={() => setHistoryOpen(false)} onSelect={(id) => { setView('all'); setSelectedId(id); }} />}
    {aiSettingsOpen && <AiSettings status={aiConfig} onChange={(status) => { setAiConfig(status); void reload(); }} onClose={() => setAiSettingsOpen(false)} />}
    {conflictOpen && <ConflictPanel onClose={() => setConflictOpen(false)} onResolved={() => { void runSync(); void reload(); }} />}
  </main>;
}
