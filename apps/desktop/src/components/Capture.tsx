import { useEffect, useRef, useState } from 'react';
import CodeMirror from '@uiw/react-codemirror';
import { markdown } from '@codemirror/lang-markdown';
import { Camera, Check, ImagePlus, Mic, MoreHorizontal, Sparkles, Square, X } from 'lucide-react';
import type { Item, SourceType } from '../types';
import { emptyContent, saveItem } from '../lib/repository';
import { Logo } from './Logo';

export function Capture({ initialType = 'text', onClose, onCreated }: { initialType?: SourceType; onClose: () => void; onCreated: (item: Item) => void }) {
  const [item, setItem] = useState<Item>(() => ({ id: crypto.randomUUID(), content: emptyContent(initialType), created_at: new Date().toISOString(), updated_at: new Date().toISOString(), version: 0, archived: false, pinned: false, sync_status: 'pending' }));
  const [saveState, setSaveState] = useState('正在创建…');
  const [recording, setRecording] = useState(initialType === 'audio');
  const [seconds, setSeconds] = useState(0);
  const first = useRef(true);

  useEffect(() => {
    const timer = window.setTimeout(() => { setSaveState('正在保存…'); saveItem(item).then((saved) => { setItem(saved); setSaveState('已保存到本地'); onCreated(saved); }); }, first.current ? 50 : 300);
    first.current = false;
    return () => window.clearTimeout(timer);
  }, [item.content, item.archived, item.pinned]);
  useEffect(() => { if (!recording) return; const timer = window.setInterval(() => setSeconds((value) => value + 1), 1000); return () => clearInterval(timer); }, [recording]);

  async function screenshot() {
    try {
      const stream = await navigator.mediaDevices.getDisplayMedia({ video: true });
      const video = document.createElement('video'); video.srcObject = stream; await video.play();
      const canvas = document.createElement('canvas'); canvas.width = video.videoWidth; canvas.height = video.videoHeight;
      canvas.getContext('2d')?.drawImage(video, 0, 0); stream.getTracks().forEach((track) => track.stop());
      setItem((current) => ({ ...current, content: { ...current.content, source_type: 'screenshot', markdown: `${current.content.markdown}\n![截图](${canvas.toDataURL('image/png')})` } }));
    } catch { /* User cancelled the system picker. */ }
  }

  return <main className="capture-shell">
    <header className="capture-head"><Logo /><div className="capture-status"><i />{saveState}</div><button className="icon-button" aria-label="关闭" onClick={onClose}><X size={18} /></button></header>
    <section className="capture-body"><input autoFocus className="capture-title" value={item.content.title} onChange={(event) => setItem({ ...item, content: { ...item.content, title: event.target.value } })} placeholder="记录标题" /><div className="capture-editor"><CodeMirror value={item.content.markdown} extensions={[markdown()]} basicSetup={{ lineNumbers: false, foldGutter: false, highlightActiveLine: false }} onChange={(value) => setItem({ ...item, content: { ...item.content, markdown: value } })} placeholder="写下此刻的想法…" /></div>
      {recording && <div className="recording-strip"><span className="record-dot" /><div className="live-wave">{Array.from({ length: 42 }, (_, i) => <i key={i} style={{ height: `${5 + ((i * 11) % 20)}px` }} />)}</div><time>{String(Math.floor(seconds / 60)).padStart(2, '0')}:{String(seconds % 60).padStart(2, '0')}</time><button className="stop-button" title="停止录音" aria-label="停止录音" onClick={() => setRecording(false)}><Square size={12} fill="currentColor" /></button></div>}
    </section>
    <footer className="capture-footer"><div className="capture-tools"><button className={recording ? 'active' : ''} onClick={() => setRecording(!recording)} title="录音"><Mic size={17} /></button><button onClick={screenshot} title="截图"><Camera size={17} /></button><button title="添加图片"><ImagePlus size={17} /></button><span /><button className="marker-quick"><Sparkles size={15} />特殊标记</button></div><div className="capture-hint"><Check size={14} />内容会自动保存</div><button className="icon-button"><MoreHorizontal size={17} /></button></footer>
  </main>;
}
