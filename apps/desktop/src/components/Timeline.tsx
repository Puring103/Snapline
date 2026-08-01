import { Archive, ArchiveRestore, AudioLines, Camera, FileText, Image, MoreHorizontal, Pin, PinOff, Search, SlidersHorizontal, Sparkles, Trash2, Video } from 'lucide-react';
import { useState } from 'react';
import type { Item, RecordFilters, SourceType } from '../types';
import { excerpt, relativeDate } from '../lib/format';

const icons = { text: FileText, screenshot: Camera, image: Image, audio: AudioLines, video: Video, mixed: FileText };

const sourceLabels: Array<[SourceType, string]> = [['text', '文本'], ['screenshot', '截图'], ['image', '图片'], ['audio', '录音'], ['video', '视频'], ['mixed', '混合']];

export function Timeline({ items, allItems, filters, onFilters, selectedId, query, onQuery, onSelect, onPin, onArchive, onDelete }: { items: Item[]; allItems: Item[]; filters: RecordFilters; onFilters: (filters: RecordFilters) => void; selectedId?: string; query: string; onQuery: (query: string) => void; onSelect: (id: string) => void; onPin: (item: Item) => void; onArchive: (item: Item) => void; onDelete: (id: string) => void }) {
  const [openMenu, setOpenMenu] = useState<string>();
  const [deleteConfirm, setDeleteConfirm] = useState<string>();
  const [filterOpen, setFilterOpen] = useState(false);
  const availableTags = [...new Set(allItems.flatMap((item) => item.content.tags))].sort();
  const availableMarkers = [...new Set(allItems.flatMap((item) => item.content.markers))].sort();
  const activeFilterCount = filters.sourceTypes.length + filters.tags.length + filters.markers.length;
  const toggle = <Key extends keyof RecordFilters>(key: Key, value: RecordFilters[Key][number]) => {
    const current = filters[key] as Array<RecordFilters[Key][number]>;
    onFilters({ ...filters, [key]: current.includes(value) ? current.filter((candidate) => candidate !== value) : [...current, value] });
  };
  return <section className="timeline">
    <header className="timeline-header"><div><h1>记录</h1><span>{items.length} 条</span></div><div className="filter-wrap"><button className={`icon-button ${activeFilterCount ? 'is-active' : ''}`} title="筛选" aria-label="筛选" aria-expanded={filterOpen} onClick={() => setFilterOpen(!filterOpen)}><SlidersHorizontal size={17} />{activeFilterCount > 0 && <em>{activeFilterCount}</em>}</button>{filterOpen && <div className="filter-menu"><header><strong>组合筛选</strong><button onClick={() => onFilters({ sourceTypes: [], tags: [], markers: [] })}>清除</button></header><fieldset><legend>来源</legend>{sourceLabels.map(([value, label]) => <label key={value}><input type="checkbox" checked={filters.sourceTypes.includes(value)} onChange={() => toggle('sourceTypes', value)} />{label}</label>)}</fieldset>{availableMarkers.length > 0 && <fieldset><legend>特殊标记</legend>{availableMarkers.map((marker) => <label key={marker}><input type="checkbox" checked={filters.markers.includes(marker)} onChange={() => toggle('markers', marker)} />{marker}</label>)}</fieldset>}{availableTags.length > 0 && <fieldset><legend>标签</legend>{availableTags.map((tag) => <label key={tag}><input type="checkbox" checked={filters.tags.includes(tag)} onChange={() => toggle('tags', tag)} />#{tag}</label>)}</fieldset>}</div>}</div></header>
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
          <div className="item-footer"><div>{item.content.markers.map((marker) => <span className="marker-chip" key={marker}>{marker}</span>)}{item.content.tags.slice(0, 2).map((tag) => <span key={tag}>#{tag}</span>)}{item.ai_status && item.ai_status !== 'complete' && <span className={`ai-job-status ${item.ai_status}`}><Sparkles size={10} />{{ unconfigured: 'AI 未配置', pending: 'AI 等待', processing: 'AI 处理中', failed: 'AI 失败' }[item.ai_status]}</span>}</div><div className="item-actions-wrap"><button className="icon-button" aria-label={`更多操作：${item.content.title || '无标题记录'}`} aria-expanded={openMenu === item.id} onClick={(event) => { event.stopPropagation(); setDeleteConfirm(undefined); setOpenMenu(openMenu === item.id ? undefined : item.id); }}><MoreHorizontal size={15} /></button>{openMenu === item.id && <div className="item-action-menu" onClick={(event) => event.stopPropagation()}>{deleteConfirm === item.id ? <div className="delete-prompt"><span>确定永久删除？</span><button onClick={() => setDeleteConfirm(undefined)}>取消</button><button className="danger" onClick={() => { setOpenMenu(undefined); onDelete(item.id); }}>确认删除</button></div> : <><button aria-label={`${item.pinned ? '取消收藏' : '收藏'}：${item.content.title || '无标题记录'}`} onClick={() => { setOpenMenu(undefined); onPin(item); }}>{item.pinned ? <PinOff size={14} /> : <Pin size={14} />}{item.pinned ? '取消收藏' : '收藏'}</button><button aria-label={`${item.archived ? '恢复' : '归档'}：${item.content.title || '无标题记录'}`} onClick={() => { setOpenMenu(undefined); onArchive(item); }}>{item.archived ? <ArchiveRestore size={14} /> : <Archive size={14} />}{item.archived ? '恢复' : '归档'}</button><button className="danger" aria-label={`删除：${item.content.title || '无标题记录'}`} onClick={() => setDeleteConfirm(item.id)}><Trash2 size={14} />删除</button></>}</div>}</div></div>
        </article></div>;
      })}
    </div>
  </section>;
}
