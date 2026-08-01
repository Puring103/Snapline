import { useEffect, useRef, useState } from 'react';
import { Camera, Check, Film, ImagePlus, Mic, MoreHorizontal, Sparkles, Square, X } from 'lucide-react';
import {
  captureNativeScreenshot,
  isTauri,
  pickAndImportAttachment,
  startNativeRecording,
  stopNativeRecording,
  storePastedImage,
  type MediaAttachment,
} from '../lib/native';
import type { Item, SourceType } from '../types';
import { emptyContent, saveItem } from '../lib/repository';
import { attachmentMarkdown } from '../lib/attachments';
import { Logo } from './Logo';

function hasContent(item: Item) {
  return Boolean(item.content.title.trim() || item.content.markdown.trim() || item.content.attachment_ids.length || item.content.tags.length || item.content.markers.length);
}

function contentSignature(item: Item) {
  return JSON.stringify([item.content, item.archived, item.pinned]);
}

function sourceTypeFor(mediaType: string): SourceType {
  if (mediaType.startsWith('audio/')) return 'audio';
  if (mediaType.startsWith('video/')) return 'video';
  return 'image';
}

function attach(item: Item, attachment: MediaAttachment, label: string, forcedType?: SourceType): Item {
  const sourceType = forcedType || sourceTypeFor(attachment.media_type);
  const reference = attachmentMarkdown(attachment.id, label, sourceType);
  return {
    ...item,
    content: {
      ...item.content,
      source_type: item.content.attachment_ids.length ? 'mixed' : sourceType,
      attachment_ids: [...item.content.attachment_ids, attachment.id],
      markdown: `${item.content.markdown}${item.content.markdown ? '\n' : ''}${reference}`,
    },
    audio_duration: attachment.duration_seconds ? `${Math.floor(attachment.duration_seconds / 60).toString().padStart(2, '0')}:${(attachment.duration_seconds % 60).toString().padStart(2, '0')}` : item.audio_duration,
  };
}

export function Capture({ initialType = 'text', onClose, onCreated }: { initialType?: SourceType; onClose: () => void; onCreated: (item: Item) => void }) {
  const [item, setItem] = useState<Item>(() => ({ id: crypto.randomUUID(), content: emptyContent(initialType), created_at: new Date().toISOString(), updated_at: new Date().toISOString(), version: 0, archived: false, pinned: false, sync_status: 'pending' }));
  const [saveState, setSaveState] = useState('等待输入');
  const [recording, setRecording] = useState(false);
  const [markerOpen, setMarkerOpen] = useState(false);
  const [seconds, setSeconds] = useState(0);
  const [error, setError] = useState('');
  const itemRef = useRef(item);
  const recordingRef = useRef(recording);
  const fileInput = useRef<HTMLInputElement>(null);
  const initialized = useRef(false);
  const lastSavedSignature = useRef('');
  const markerOptions = ['账目', '重要', '待办', '稍后处理', '需跟进'];
  itemRef.current = item;
  recordingRef.current = recording;

  async function saveNow(candidate = itemRef.current) {
    if (!hasContent(candidate)) return candidate;
    setSaveState('正在保存…');
    const saved = await saveItem(candidate);
    lastSavedSignature.current = contentSignature(saved);
    itemRef.current = saved;
    setItem(saved);
    setSaveState('已保存到本地');
    onCreated(saved);
    return saved;
  }

  useEffect(() => {
    if (!hasContent(item)) return;
    if (contentSignature(item) === lastSavedSignature.current) return;
    const timer = window.setTimeout(() => { void saveNow(item).catch(() => setSaveState('保存失败，正在重试')); }, 300);
    return () => window.clearTimeout(timer);
  }, [item.content, item.archived, item.pinned]);

  useEffect(() => {
    if (!recording) return;
    const timer = window.setInterval(() => setSeconds((value) => value + 1), 1000);
    return () => clearInterval(timer);
  }, [recording]);

  async function beginRecording() {
    setError('');
    try {
      if (isTauri()) await startNativeRecording();
      setSeconds(0);
      setRecording(true);
    } catch (reason) {
      setError(typeof reason === 'string' ? reason : '无法开始录音');
    }
  }

  async function finishRecording() {
    if (!recordingRef.current) return itemRef.current;
    setError('');
    try {
      if (!isTauri()) {
        setRecording(false);
        return itemRef.current;
      }
      const attachment = await stopNativeRecording();
      const changed = attach(itemRef.current, attachment, attachment.display_name, 'audio');
      itemRef.current = changed;
      setItem(changed);
      setRecording(false);
      return changed;
    } catch (reason) {
      setRecording(false);
      setError(typeof reason === 'string' ? reason : '无法保存录音');
      return itemRef.current;
    }
  }

  async function screenshot() {
    setError('');
    try {
      if (isTauri()) {
        const attachment = await captureNativeScreenshot();
        setItem((current) => attach(current, attachment, attachment.display_name, 'screenshot'));
        return;
      }
      const stream = await navigator.mediaDevices.getDisplayMedia({ video: true });
      const video = document.createElement('video');
      video.srcObject = stream;
      await video.play();
      const canvas = document.createElement('canvas');
      canvas.width = video.videoWidth;
      canvas.height = video.videoHeight;
      canvas.getContext('2d')?.drawImage(video, 0, 0);
      stream.getTracks().forEach((track) => track.stop());
      setItem((current) => ({ ...current, content: { ...current.content, source_type: 'screenshot', markdown: `${current.content.markdown}\n![截图](${canvas.toDataURL('image/png')})` } }));
    } catch (reason) {
      if (isTauri()) setError(typeof reason === 'string' ? reason : '截图失败');
    }
  }

  async function importFile() {
    if (!isTauri()) {
      fileInput.current?.click();
      return;
    }
    try {
      const attachment = await pickAndImportAttachment();
      if (attachment) setItem((current) => attach(current, attachment, attachment.display_name));
    } catch (reason) {
      setError(typeof reason === 'string' ? reason : '无法导入附件');
    }
  }

  async function addImage(file?: File) {
    if (!file) return;
    try {
      if (isTauri()) {
        const attachment = await storePastedImage(file);
        setItem((current) => attach(current, attachment, file.name || '粘贴的图片', 'image'));
      } else {
        const reader = new FileReader();
        reader.onload = () => setItem((current) => ({ ...current, content: { ...current.content, source_type: 'image', markdown: `${current.content.markdown}\n![${file.name || '图片'}](${String(reader.result)})` } }));
        reader.readAsDataURL(file);
      }
    } catch (reason) {
      setError(typeof reason === 'string' ? reason : '无法保存图片');
    }
  }

  async function closeCapture() {
    const completed = await finishRecording();
    await saveNow(completed).catch(() => undefined);
    if (isTauri()) {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const currentWindow = getCurrentWindow();
      if (currentWindow.label === 'capture') await currentWindow.destroy();
      else onClose();
    } else {
      onClose();
    }
  }

  useEffect(() => {
    if (initialized.current) return;
    initialized.current = true;
    if (initialType === 'audio') void beginRecording();
    if (initialType === 'screenshot') void screenshot();
  }, [initialType]);

  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    void import('@tauri-apps/api/window').then(async ({ getCurrentWindow }) => {
      const currentWindow = getCurrentWindow();
      if (currentWindow.label !== 'capture') return;
      unlisten = await currentWindow.onCloseRequested(async (event) => {
        event.preventDefault();
        await closeCapture();
      });
    });
    return () => unlisten?.();
  }, []);

  function paste(event: React.ClipboardEvent) {
    const image = [...event.clipboardData.files].find((file) => file.type.startsWith('image/'));
    if (image) {
      event.preventDefault();
      void addImage(image);
    }
  }

  return <main className="capture-shell" onPaste={paste}>
    <header className="capture-head"><Logo /><div className="capture-status"><i />{saveState}</div><button className="icon-button" aria-label="关闭" onClick={() => void closeCapture()}><X size={18} /></button></header>
    <section className="capture-body"><input autoFocus className="capture-title" value={item.content.title} onChange={(event) => setItem({ ...item, content: { ...item.content, title: event.target.value } })} placeholder="记录标题" /><div className="capture-editor"><textarea value={item.content.markdown} onChange={(event) => setItem({ ...item, content: { ...item.content, markdown: event.target.value } })} placeholder={initialType === 'image' ? '直接粘贴图片，也可以补充文字…' : '写下此刻的想法…'} /></div>
      {error && <div className="capture-error" role="alert">{error}</div>}
      {recording && <div className="recording-strip"><span className="record-dot" /><div className="live-wave">{Array.from({ length: 42 }, (_, i) => <i key={i} style={{ height: `${5 + ((i * 11) % 20)}px` }} />)}</div><time>{String(Math.floor(seconds / 60)).padStart(2, '0')}:{String(seconds % 60).padStart(2, '0')}</time><button className="stop-button" title="停止录音" aria-label="停止录音" onClick={() => void finishRecording()}><Square size={12} fill="currentColor" /></button></div>}
    </section>
    <footer className="capture-footer"><div className="capture-tools"><button className={recording ? 'active' : ''} onClick={() => void (recording ? finishRecording() : beginRecording())} title="录音"><Mic size={17} /></button><button onClick={() => void screenshot()} title="截图"><Camera size={17} /></button><button onClick={() => fileInput.current?.click()} title="添加图片"><ImagePlus size={17} /></button><button onClick={() => void importFile()} title="导入图片或视频"><Film size={17} /></button><input ref={fileInput} className="visually-hidden" type="file" accept="image/*" onChange={(event) => { void addImage(event.target.files?.[0]); event.currentTarget.value = ''; }} /><span /><div className="capture-marker-wrap"><button className="marker-quick" aria-expanded={markerOpen} onClick={() => setMarkerOpen(!markerOpen)}><Sparkles size={15} />特殊标记</button>{markerOpen && <div className="marker-menu capture-marker-menu">{markerOptions.map((marker) => <button key={marker} onClick={() => { if (!item.content.markers.includes(marker)) setItem({ ...item, content: { ...item.content, markers: [...item.content.markers, marker] } }); setMarkerOpen(false); }}><span className="marker-dot" />{marker}{item.content.markers.includes(marker) && <Check size={14} />}</button>)}</div>}</div></div><div className="capture-hint"><Check size={14} />内容会自动保存</div><button className="icon-button" title="更多" aria-label="更多"><MoreHorizontal size={17} /></button></footer>
  </main>;
}
