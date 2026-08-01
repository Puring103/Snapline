import { Archive, AudioLines, Camera, FileText, Image, MoreHorizontal, Pin, Search, SlidersHorizontal, Video } from 'lucide-react';
import type { Item } from '../types';
import { excerpt, relativeDate } from '../lib/format';

const icons = { text: FileText, screenshot: Camera, image: Image, audio: AudioLines, video: Video, mixed: FileText };

export function Timeline({ items, selectedId, query, onQuery, onSelect }: { items: Item[]; selectedId?: string; query: string; onQuery: (query: string) => void; onSelect: (id: string) => void }) {
  return <section className="timeline">
    <header className="timeline-header"><div><h1>记录</h1><span>{items.length} 条</span></div><button className="icon-button" title="筛选" aria-label="筛选"><SlidersHorizontal size={17} /></button></header>
    <div className="search-field"><Search size={16} /><input value={query} onChange={(event) => onQuery(event.target.value)} placeholder="搜索记录、标签或标记" /><kbd>⌘ K</kbd></div>
    <div className="timeline-scroll">
      {items.length === 0 && <div className="empty-state"><Search size={22} /><strong>没有找到记录</strong><span>换一个关键词或筛选条件</span></div>}
      {items.map((item, index) => {
        const Icon = icons[item.content.source_type];
        const dayBreak = index === 0 || new Date(items[index - 1].updated_at).toDateString() !== new Date(item.updated_at).toDateString();
        return <div key={item.id}>{dayBreak && <div className="day-label">{relativeDate(item.updated_at).split(' ')[0]}</div>}<article className={`timeline-item ${selectedId === item.id ? 'selected' : ''}`} onClick={() => onSelect(item.id)}>
          <div className="item-meta"><span><Icon size={14} />{item.content.source_type === 'audio' ? `录音 ${item.audio_duration || ''}` : item.content.source_type === 'screenshot' ? '截图' : item.content.source_type === 'image' ? '图片' : '文本'}</span><time>{relativeDate(item.updated_at).split(' ').slice(1).join(' ')}</time></div>
          <div className="item-title-row"><h2>{item.content.title || '无标题记录'}</h2>{item.pinned && <Pin size={13} className="pinned-icon" />}</div>
          {item.preview_image && <img className="item-thumbnail" src={item.preview_image} alt="记录附件预览" />}
          {item.content.source_type === 'audio' && <div className="mini-wave">{Array.from({ length: 24 }, (_, i) => <i key={i} style={{ height: `${4 + ((i * 9) % 14)}px` }} />)}</div>}
          <p>{item.content.ai_metadata?.summary || excerpt(item.content.markdown) || '等待输入内容'}</p>
          <div className="item-footer"><div>{item.content.markers.map((marker) => <span className="marker-chip" key={marker}>{marker}</span>)}{item.content.tags.slice(0, 2).map((tag) => <span key={tag}>#{tag}</span>)}</div><button className="icon-button" aria-label="更多操作"><MoreHorizontal size={15} /></button></div>
        </article></div>;
      })}
    </div>
  </section>;
}
