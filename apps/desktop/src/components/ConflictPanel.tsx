import { useEffect, useState } from 'react';
import { GitCompareArrows, X } from 'lucide-react';
import { listSyncConflicts, resolveSyncConflict, type SyncConflict } from '../lib/native';

export function ConflictPanel({ onClose, onResolved }: { onClose: () => void; onResolved: () => void }) {
  const [conflicts, setConflicts] = useState<SyncConflict[]>([]);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState('');
  useEffect(() => { void listSyncConflicts().then(setConflicts).catch((cause) => setError(cause instanceof Error ? cause.message : '无法读取同步冲突')); }, []);
  const resolve = async (conflict: SyncConflict, choice: 'local' | 'remote') => {
    setBusy(conflict.object_id); setError('');
    try {
      await resolveSyncConflict(conflict.object_id, choice);
      setConflicts((current) => current.filter((item) => item.object_id !== conflict.object_id));
      onResolved();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : '无法处理同步冲突');
    } finally { setBusy(''); }
  };
  return <div className="settings-backdrop"><section className="conflict-dialog" role="dialog" aria-label="同步冲突">
    <header><div><span><GitCompareArrows size={17} /></span><div><strong>同步冲突</strong><small>逐条选择要保留的版本</small></div></div><button className="icon-button" onClick={onClose} aria-label="关闭同步冲突"><X size={18} /></button></header>
    <div className="conflict-list">{!conflicts.length && !error && <p className="conflict-empty">没有待处理的冲突</p>}{conflicts.map((conflict) => <article key={conflict.object_id} className="conflict-row"><div><small>本地版本</small><strong>{conflict.local?.content.title || '本地已删除'}</strong><p>{conflict.local?.content.markdown || '这条记录已在本地删除。'}</p><button disabled={busy === conflict.object_id} onClick={() => void resolve(conflict, 'local')}>保留本地</button></div><div><small>云端版本</small><strong>{conflict.remote?.content.title || '云端已删除'}</strong><p>{conflict.remote?.content.markdown || '这条记录已在云端删除。'}</p><button disabled={busy === conflict.object_id} onClick={() => void resolve(conflict, 'remote')}>采用云端</button></div></article>)}</div>
    {error && <div className="settings-error" role="alert">{error}</div>}
  </section></div>;
}
