import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it } from 'vitest';
import type { Item } from '../types';
import { HistoryPanel } from './HistoryPanel';

function item(index: number): Item {
  return {
    id: `00000000-0000-4000-8000-${index.toString().padStart(12, '0')}`,
    content: { title: `记录 ${index}`, markdown: `正文 ${index}`, source_type: 'text', tags: [], markers: [], attachment_ids: [], ai_metadata: null },
    created_at: new Date(2026, 0, 1, 0, index).toISOString(),
    updated_at: new Date(2026, 0, 1, 0, index).toISOString(),
    version: 1,
    archived: false,
    pinned: false,
    sync_status: 'pending',
  };
}

it('paginates large history lists in stable batches', () => {
  render(<HistoryPanel items={Array.from({ length: 120 }, (_, index) => item(index))} onClose={() => undefined} onSelect={() => undefined} />);
  expect(screen.getAllByRole('button', { name: /记录 \d+/ })).toHaveLength(50);
  fireEvent.click(screen.getByRole('button', { name: '加载更多' }));
  expect(screen.getAllByRole('button', { name: /记录 \d+/ })).toHaveLength(100);
});
