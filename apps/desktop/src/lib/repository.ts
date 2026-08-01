import { initialItems } from '../data';
import type { Item, ItemContent } from '../types';
import { isTauri } from './native';

const KEY = 'snapline-dev-items-v1';
async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const api = await import('@tauri-apps/api/core');
  return api.invoke<T>(command, args);
}

export async function listItems(): Promise<Item[]> {
  if (isTauri()) return invoke<Item[]>('list_items');
  const value = localStorage.getItem(KEY);
  if (!value) {
    localStorage.setItem(KEY, JSON.stringify(initialItems));
    return initialItems;
  }
  return JSON.parse(value) as Item[];
}

export async function saveItem(item: Item): Promise<Item> {
  if (isTauri()) return invoke<Item>('save_item', { input: { id: item.id, content: item.content, archived: item.archived, pinned: item.pinned } });
  const items = await listItems();
  const now = new Date().toISOString();
  const existing = items.find((candidate) => candidate.id === item.id);
  const saved = { ...item, updated_at: now, version: existing ? existing.version + 1 : 1, sync_status: 'pending' };
  localStorage.setItem(KEY, JSON.stringify([saved, ...items.filter((candidate) => candidate.id !== item.id)]));
  return saved;
}

export async function deleteItem(id: string): Promise<void> {
  if (isTauri()) return invoke<void>('delete_item', { id });
  localStorage.setItem(KEY, JSON.stringify((await listItems()).filter((item) => item.id !== id)));
}

export function emptyContent(type: ItemContent['source_type'] = 'text'): ItemContent {
  return { title: '', markdown: '', source_type: type, tags: [], markers: [], attachment_ids: [], ai_metadata: null };
}
