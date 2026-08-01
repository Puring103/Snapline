import { useEffect, useMemo, useRef, useState } from 'react';
import CodeMirror from '@uiw/react-codemirror';
import { markdown } from '@codemirror/lang-markdown';
import { Archive, Bold, Check, Code2, Heading2, ImagePlus, Italic, Link, List, ListChecks, Mic, MoreHorizontal, Pin, Quote, Redo2, Sparkles, Trash2, Undo2, X } from 'lucide-react';
import type { Item } from '../types';

type SaveState = 'saved' | 'saving' | 'error';

export function EditorPane({ item, onChange, onSave, onDelete }: { item?: Item; onChange: (item: Item) => void; onSave: (item: Item) => Promise<void>; onDelete: (id: string) => void }) {
  const [saveState, setSaveState] = useState<SaveState>('saved');
  const [tagInput, setTagInput] = useState('');
  const [markerOpen, setMarkerOpen] = useState(false);
  const skipFirst = useRef(true);

  useEffect(() => { skipFirst.current = true; setSaveState('saved'); }, [item?.id]);
  useEffect(() => {
    if (!item || skipFirst.current) { skipFirst.current = false; return; }
    setSaveState('saving');
    const timer = window.setTimeout(() => onSave(item).then(() => setSaveState('saved')).catch(() => setSaveState('error')), 300);
    return () => window.clearTimeout(timer);
  }, [item, onSave]);

  const extensions = useMemo(() => [markdown()], []);
  if (!item) return <section className="editor-pane empty-editor"><Sparkles size={24} /><strong>选择一条记录</strong><span>在时间线中选择记录，或创建新的记录。</span></section>;

  const currentItem = item;
  const updateContent = (patch: Partial<Item['content']>) => onChange({ ...currentItem, content: { ...currentItem.content, ...patch } });
  const insert = (before: string, after = '') => updateContent({ markdown: `${currentItem.content.markdown}${currentItem.content.markdown ? '\n' : ''}${before}${after}` });
  const markerOptions = ['账目', '重要', '待办', '稍后处理', '需跟进'];

  function addTag(event: React.KeyboardEvent<HTMLInputElement>) {
    if ((event.key === 'Enter' || event.key === ',') && tagInput.trim()) {
      event.preventDefault();
      if (!currentItem.content.tags.includes(tagInput.trim())) updateContent({ tags: [...currentItem.content.tags, tagInput.trim()] });
      setTagInput('');
    }
  }

  function paste(event: React.ClipboardEvent) {
    const image = [...event.clipboardData.files].find((file) => file.type.startsWith('image/'));
    if (!image) return;
    event.preventDefault();
    const reader = new FileReader();
    reader.onload = () => insert(`![粘贴的图片](${String(reader.result)})`);
    reader.readAsDataURL(image);
  }

  return <section className="editor-pane">
    <header className="editor-head"><div className={`save-state ${saveState}`}><i />{saveState === 'saved' ? '已保存到本地' : saveState === 'saving' ? '正在保存…' : '保存失败，正在重试'}</div><div className="editor-actions"><button className={`icon-button ${item.pinned ? 'is-active' : ''}`} title="收藏" aria-label="收藏" onClick={() => onChange({ ...item, pinned: !item.pinned })}><Pin size={17} /></button><button className="icon-button" title="归档" aria-label="归档" onClick={() => onChange({ ...item, archived: true })}><Archive size={17} /></button><button className="icon-button" title="更多" aria-label="更多"><MoreHorizontal size={18} /></button></div></header>
    <div className="editor-title-wrap"><input className="editor-title" value={item.content.title} onChange={(event) => updateContent({ title: event.target.value })} placeholder="无标题记录" /><div className="record-time">创建于 {new Date(item.created_at).toLocaleString('zh-CN', { month: 'long', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false })}</div></div>
    <div className="editor-toolbar" role="toolbar" aria-label="Markdown 格式"><button title="撤销" aria-label="撤销"><Undo2 size={16} /></button><button title="重做" aria-label="重做"><Redo2 size={16} /></button><span /><button title="二级标题" aria-label="二级标题" onClick={() => insert('## ')}><Heading2 size={16} /></button><button title="粗体" aria-label="粗体" onClick={() => insert('**粗体**')}><Bold size={16} /></button><button title="斜体" aria-label="斜体" onClick={() => insert('*斜体*')}><Italic size={16} /></button><button title="引用" aria-label="引用" onClick={() => insert('> ')}><Quote size={16} /></button><button title="列表" aria-label="列表" onClick={() => insert('- ')}><List size={16} /></button><button title="任务列表" aria-label="任务列表" onClick={() => insert('- [ ] ')}><ListChecks size={16} /></button><button title="代码" aria-label="代码" onClick={() => insert('```\n\n```')}><Code2 size={16} /></button><button title="链接" aria-label="链接" onClick={() => insert('[链接](https://)')}><Link size={16} /></button><span /><button title="添加图片" aria-label="添加图片"><ImagePlus size={16} /></button><button title="录音" aria-label="录音"><Mic size={16} /></button></div>
    <div className="code-editor" onPaste={paste}><CodeMirror value={item.content.markdown} extensions={extensions} basicSetup={{ lineNumbers: false, foldGutter: false, highlightActiveLine: false, highlightActiveLineGutter: false }} onChange={(markdownValue) => updateContent({ markdown: markdownValue })} placeholder="开始记录…支持 Markdown，也可以直接粘贴图片。" /></div>
    <footer className="editor-footer"><div className="tag-editor">{item.content.markers.map((marker) => <span className="marker-chip" key={marker}>{marker}<button aria-label={`移除 ${marker}`} onClick={() => updateContent({ markers: item.content.markers.filter((value) => value !== marker) })}><X size={11} /></button></span>)}{item.content.tags.map((tag) => <span className="tag-chip" key={tag}>#{tag}<button aria-label={`移除 ${tag}`} onClick={() => updateContent({ tags: item.content.tags.filter((value) => value !== tag) })}><X size={11} /></button></span>)}<input value={tagInput} onChange={(event) => setTagInput(event.target.value)} onKeyDown={addTag} placeholder="添加标签…" /></div><div className="marker-picker-wrap"><button className="secondary-button" onClick={() => setMarkerOpen(!markerOpen)}><Sparkles size={14} />特殊标记</button>{markerOpen && <div className="marker-menu">{markerOptions.map((marker) => <button key={marker} onClick={() => { if (!item.content.markers.includes(marker)) updateContent({ markers: [...item.content.markers, marker] }); setMarkerOpen(false); }}><span className="marker-dot" />{marker}{item.content.markers.includes(marker) && <Check size={14} />}</button>)}</div>}<button className="icon-button danger-action" title="删除记录" aria-label="删除记录" onClick={() => onDelete(item.id)}><Trash2 size={16} /></button></div></footer>
  </section>;
}
