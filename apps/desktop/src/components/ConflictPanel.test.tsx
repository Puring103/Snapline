import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { ConflictPanel } from './ConflictPanel';
import { listSyncConflicts, resolveSyncConflict } from '../lib/native';

vi.mock('../lib/native', () => ({ listSyncConflicts: vi.fn(), resolveSyncConflict: vi.fn() }));

it('shows both encrypted record versions and resolves with an explicit choice', async () => {
  vi.mocked(listSyncConflicts).mockResolvedValue([{ object_id: 'record-1', local: { id: 'record-1', content: { title: '本地标题', markdown: '本地正文', source_type: 'text', tags: [], markers: [], attachment_ids: [], ai_metadata: null }, created_at: '', updated_at: '', version: 2, archived: false, pinned: false, sync_status: 'conflict' }, remote: { id: 'record-1', content: { title: '云端标题', markdown: '云端正文', source_type: 'text', tags: [], markers: [], attachment_ids: [], ai_metadata: null }, created_at: '', updated_at: '', version: 3, archived: false, pinned: false, sync_status: 'conflict' }, remote_deleted: false, remote_version: 3 }]);
  vi.mocked(resolveSyncConflict).mockResolvedValue();
  const onResolved = vi.fn();
  render(<ConflictPanel onClose={vi.fn()} onResolved={onResolved} />);
  expect(await screen.findByText('本地标题')).toBeInTheDocument();
  expect(screen.getByText('云端标题')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: '采用云端' }));
  await screen.findByText('没有待处理的冲突');
  expect(resolveSyncConflict).toHaveBeenCalledWith('record-1', 'remote');
  expect(onResolved).toHaveBeenCalledOnce();
});
