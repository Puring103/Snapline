import { useEffect, useMemo, useRef, useState } from 'react';
import CodeMirror from '@uiw/react-codemirror';
import { markdown } from '@codemirror/lang-markdown';
import { redo, undo } from '@codemirror/commands';
import type { EditorView } from '@codemirror/view';
import { Archive, Bold, Check, Code2, Heading2, ImagePlus, Italic, Link, List, ListChecks, Mic, MoreHorizontal, Pin, Quote, Redo2, Sparkles, Trash2, Undo2, X } from 'lucide-react';
import { isTauri, pickAndImportAttachment, storePastedImage } from '../lib/native';
import { attachmentMarkdown } from '../lib/attachments';
import type { Item } from '../types';
import { AttachmentPreview } from './AttachmentPreview';

type SaveState = 'saved' | 'saving' | 'error';

function contentSignature(item: Item) {
  return JSON.stringify([item.content, item.archived, item.pinned]);
}

export function EditorPane({ item, onChange, onSave, onDelete, onRecord }: { item?: Item; onChange: (item: Item) => void; onSave: (item: Item) => Promise<void>; onDelete: (id: string) => void; onRecord: () => void }) {
  const [saveState, setSaveState] = useState<SaveState>('saved');
  const [tagInput, setTagInput] = useState('');
  const [markerOpen, setMarkerOpen] = useState(false);
  const skipFirst = useRef(true);
  const lastPersistedSignature = useRef('');
  const editorView = useRef<EditorView | null>(null);

  useEffect(() => { skipFirst.current = true; lastPersistedSignature.current = item ? contentSignature(item) : ''; setSaveState('saved'); }, [item?.id]);
  useEffect(() => {
    if (!item || skipFirst.current) { skipFirst.current = false; return; }
    const signature = contentSignature(item);
    if (signature === lastPersistedSignature.current) return;
    setSaveState('saving');
    const timer = window.setTimeout(() => {
      lastPersistedSignature.current = signature;
      onSave(item).then(() => setSaveState('saved')).catch(() => { lastPersistedSignature.current = ''; setSaveState('error'); });
    }, 300);
    return () => window.clearTimeout(timer);
  }, [item, onSave]);

  const extensions = useMemo(() => [markdown()], []);
  if (!item) return <section className="editor-pane empty-editor"><Sparkles size={24} /><strong>选择一条记录</strong><span>在时间线中选择记录，或创建新的记录。</span></section>;

  const currentItem = item;
  const updateContent = (patch: Partial<Item['content']>) => onChange({ ...currentItem, content: { ...currentItem.content, ...patch } });
  const insert = (before: string, after = '') => updateContent({ markdown: `${currentItem.content.markdown}${currentItem.content.markdown ? '\n' : ''}${before}${after}` });
  const markerOptions = ['账目', '重要', '待办', '稍后处理', '需跟进'];

  function wrapSelection(before: string, after: string, placeholder: string) {
    const view = editorView.current;
    if (!view) return;
    const { from, to } = view.state.selection.main;
    const selected = view.state.sliceDoc(from, to) || placeholder;
    view.dispatch({
      changes: { from, to, insert: `${before}${selected}${after}` },
      selection: { anchor: from + before.length, head: from + before.length + selected.length },
    });
    view.focus();
  }

  function prefixSelectedLines(prefix: string) {
    const view = editorView.current;
    if (!view) return;
    const { from, to } = view.state.selection.main;
    const firstLine = view.state.doc.lineAt(from);
    const lastLine = view.state.doc.lineAt(to);
    const changes: Array<{ from: number; insert: string }> = [];
    for (let lineNumber = firstLine.number; lineNumber <= lastLine.number; lineNumber += 1) {
      changes.push({ from: view.state.doc.line(lineNumber).from, insert: prefix });
    }
    view.dispatch({ changes });
    view.focus();
  }

  function addTag(event: React.KeyboardEvent<HTMLInputElement>) {
    if ((event.key === 'Enter' || event.key === ',') && tagInput.trim()) {
      event.preventDefault();
      if (!currentItem.content.tags.includes(tagInput.trim())) updateContent({ tags: [...currentItem.content.tags, tagInput.trim()] });
      setTagInput('');
    }
  }

  async function paste(event: React.ClipboardEvent) {
    const image = [...event.clipboardData.files].find((file) => file.type.startsWith('image/'));
    if (!image) return;
    event.preventDefault();
    if (isTauri()) {
      const attachment = await storePastedImage(image);
      onChange({ ...currentItem, content: { ...currentItem.content, source_type: currentItem.content.attachment_ids.length ? 'mixed' : 'image', attachment_ids: [...currentItem.content.attachment_ids, attachment.id], markdown: `${currentItem.content.markdown}${currentItem.content.markdown ? '\n' : ''}${attachmentMarkdown(attachment.id, attachment.display_name, 'image')}` } });
      return;
    }
    const reader = new FileReader();
    reader.onload = () => insert(`![粘贴的图片](${String(reader.result)})`);
    reader.readAsDataURL(image);
  }

  async function importAttachment() {
    if (!isTauri()) return;
    const attachment = await pickAndImportAttachment();
    if (!attachment) return;
    const sourceType = attachment.media_type.startsWith('video/') ? 'video' : 'image';
    const reference = attachmentMarkdown(attachment.id, attachment.display_name, sourceType);
    onChange({ ...currentItem, content: { ...currentItem.content, source_type: currentItem.content.attachment_ids.length ? 'mixed' : sourceType, attachment_ids: [...currentItem.content.attachment_ids, attachment.id], markdown: `${currentItem.content.markdown}${currentItem.content.markdown ? '\n' : ''}${reference}` } });
  }

  return <section className="editor-pane">
    <header className="editor-head"><div className={`save-state ${saveState}`}><i />{saveState === 'saved' ? '已保存到本地' : saveState === 'saving' ? '正在保存…' : '保存失败，正在重试'}</div><div className="editor-actions"><button className={`icon-button ${item.pinned ? 'is-active' : ''}`} title="收藏" aria-label="收藏" onClick={() => onChange({ ...item, pinned: !item.pinned })}><Pin size={17} /></button><button className="icon-button" title="归档" aria-label="归档" onClick={() => onChange({ ...item, archived: true })}><Archive size={17} /></button><button className="icon-button" title="更多" aria-label="更多"><MoreHorizontal size={18} /></button></div></header>
    <div className="editor-title-wrap"><input className="editor-title" value={item.content.title} onChange={(event) => updateContent({ title: event.target.value })} placeholder="无标题记录" /><div className="record-time">创建于 {new Date(item.created_at).toLocaleString('zh-CN', { month: 'long', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false })}</div></div>
    <div className="editor-toolbar" role="toolbar" aria-label="Markdown 格式"><button title="撤销" aria-label="撤销" onClick={() => { if (editorView.current) undo(editorView.current); }}><Undo2 size={16} /></button><button title="重做" aria-label="重做" onClick={() => { if (editorView.current) redo(editorView.current); }}><Redo2 size={16} /></button><span /><button title="二级标题" aria-label="二级标题" onClick={() => prefixSelectedLines('## ')}><Heading2 size={16} /></button><button title="粗体" aria-label="粗体" onClick={() => wrapSelection('**', '**', '粗体')}><Bold size={16} /></button><button title="斜体" aria-label="斜体" onClick={() => wrapSelection('*', '*', '斜体')}><Italic size={16} /></button><button title="引用" aria-label="引用" onClick={() => prefixSelectedLines('> ')}><Quote size={16} /></button><button title="列表" aria-label="列表" onClick={() => prefixSelectedLines('- ')}><List size={16} /></button><button title="任务列表" aria-label="任务列表" onClick={() => prefixSelectedLines('- [ ] ')}><ListChecks size={16} /></button><button title="代码" aria-label="代码" onClick={() => wrapSelection('`', '`', '代码')}><Code2 size={16} /></button><button title="链接" aria-label="链接" onClick={() => wrapSelection('[', '](https://)', '链接')}><Link size={16} /></button><span /><button title="添加图片或视频" aria-label="添加图片或视频" onClick={() => void importAttachment()}><ImagePlus size={16} /></button><button title="录音" aria-label="录音" onClick={onRecord}><Mic size={16} /></button></div>
    <div className="code-editor" onPaste={(event) => void paste(event)}><CodeMirror value={item.content.markdown} extensions={extensions} basicSetup={{ lineNumbers: false, foldGutter: false, highlightActiveLine: false, highlightActiveLineGutter: false }} onCreateEditor={(view) => { editorView.current = view; }} onChange={(markdownValue) => updateContent({ markdown: markdownValue })} placeholder="开始记录…支持 Markdown，也可以直接粘贴图片。" /><AttachmentPreview item={item} /></div>
    <footer className="editor-footer"><div className="tag-editor">{item.content.markers.map((marker) => <span className="marker-chip" key={marker}>{marker}<button aria-label={`移除 ${marker}`} onClick={() => updateContent({ markers: item.content.markers.filter((value) => value !== marker) })}><X size={11} /></button></span>)}{item.content.tags.map((tag) => <span className="tag-chip" key={tag}>#{tag}<button aria-label={`移除 ${tag}`} onClick={() => updateContent({ tags: item.content.tags.filter((value) => value !== tag) })}><X size={11} /></button></span>)}<input value={tagInput} onChange={(event) => setTagInput(event.target.value)} onKeyDown={addTag} placeholder="添加标签…" /></div><div className="marker-picker-wrap"><button className="secondary-button" onClick={() => setMarkerOpen(!markerOpen)}><Sparkles size={14} />特殊标记</button>{markerOpen && <div className="marker-menu">{markerOptions.map((marker) => <button key={marker} onClick={() => { if (!item.content.markers.includes(marker)) updateContent({ markers: [...item.content.markers, marker] }); setMarkerOpen(false); }}><span className="marker-dot" />{marker}{item.content.markers.includes(marker) && <Check size={14} />}</button>)}</div>}<button className="icon-button danger-action" title="删除记录" aria-label="删除记录" onClick={() => onDelete(item.id)}><Trash2 size={16} /></button></div></footer>
  </section>;
}

export default EditorPane;
