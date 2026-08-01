import { useState } from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import type { Item } from '../types';
import { EditorPane } from './EditorPane';

const initialItem: Item = {
  id: 'test-item',
  content: { title: '', markdown: '', source_type: 'text', tags: [], markers: [], attachment_ids: [], ai_metadata: null },
  created_at: '2026-08-01T00:00:00.000Z',
  updated_at: '2026-08-01T00:00:00.000Z',
  version: 1,
  archived: false,
  pinned: false,
  sync_status: 'pending',
};

it('does not enter an automatic save loop when native persistence returns a cloned item', async () => {
  const saves = vi.fn();
  function Harness() {
    const [item, setItem] = useState(initialItem);
    return <EditorPane item={item} onChange={setItem} onDelete={() => undefined} onRecord={() => undefined} onSave={async (changed) => {
      saves(changed);
      setItem(JSON.parse(JSON.stringify({ ...changed, version: changed.version + 1 })) as Item);
    }} />;
  }

  render(<Harness />);
  fireEvent.change(screen.getByPlaceholderText('无标题记录'), { target: { value: '只保存一次' } });
  await waitFor(() => expect(saves).toHaveBeenCalledTimes(1));
  await new Promise((resolve) => window.setTimeout(resolve, 700));
  expect(saves).toHaveBeenCalledTimes(1);
});

it('applies Markdown formatting at the cursor and uses the editor history', async () => {
  function Harness() {
    const [item, setItem] = useState(initialItem);
    return <>
      <output data-testid="markdown">{item.content.markdown}</output>
      <EditorPane item={item} onChange={setItem} onDelete={() => undefined} onRecord={() => undefined} onSave={async () => undefined} />
    </>;
  }

  render(<Harness />);
  fireEvent.click(screen.getByRole('button', { name: '粗体' }));
  await waitFor(() => expect(screen.getByTestId('markdown')).toHaveTextContent('**粗体**'));

  fireEvent.click(screen.getByRole('button', { name: '撤销' }));
  await waitFor(() => expect(screen.getByTestId('markdown')).toBeEmptyDOMElement());

  fireEvent.click(screen.getByRole('button', { name: '二级标题' }));
  await waitFor(() => expect(screen.getByTestId('markdown')).toHaveTextContent('##'));
});
