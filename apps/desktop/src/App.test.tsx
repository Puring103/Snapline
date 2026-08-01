import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { App } from './App';

beforeEach(() => {
  localStorage.clear();
  localStorage.setItem('snapline-dev-session', 'authenticated');
  history.replaceState(null, '', '/');
});

describe('desktop workspace', () => {
  it('creates and automatically saves a record without a save button', async () => {
    render(<App />);
    await screen.findByText('关于新产品首页的想法');
    fireEvent.click(screen.getByRole('button', { name: /新记录/ }));
    const title = await screen.findByPlaceholderText('无标题记录');
    fireEvent.change(title, { target: { value: '自动保存测试' } });
    expect(screen.queryByRole('button', { name: '保存' })).not.toBeInTheDocument();
    await waitFor(() => expect(localStorage.getItem('snapline-dev-items-v1')).toContain('自动保存测试'));
  });

  it('filters by the built-in account marker without an accounting module', async () => {
    render(<App />);
    await screen.findByText('团队午餐票据');
    fireEvent.click(screen.getByRole('button', { name: '账目' }));
    expect(screen.getByText('团队午餐票据')).toBeInTheDocument();
    expect(screen.queryByText('散步时的语音想法')).not.toBeInTheDocument();
  });

  it('shows only the record editor in the quick capture entry and skips empty drafts', async () => {
    history.replaceState(null, '', '/?capture=text');
    render(<App />);
    await screen.findByPlaceholderText('记录标题');
    expect(screen.queryByRole('button', { name: '历史记录' })).not.toBeInTheDocument();
    expect(screen.queryByText('全部记录')).not.toBeInTheDocument();
    await new Promise((resolve) => window.setTimeout(resolve, 450));
    const stored = JSON.parse(localStorage.getItem('snapline-dev-items-v1') || '[]') as unknown[];
    expect(stored).toHaveLength(4);
  });

  it('automatically saves quick capture content once it becomes meaningful', async () => {
    history.replaceState(null, '', '/?capture=text');
    render(<App />);
    const title = await screen.findByPlaceholderText('记录标题');
    fireEvent.change(title, { target: { value: '快捷记录自动保存' } });
    await waitFor(() => expect(localStorage.getItem('snapline-dev-items-v1')).toContain('快捷记录自动保存'));
    const saved = (JSON.parse(localStorage.getItem('snapline-dev-items-v1') || '[]') as Array<{ content: { title: string }; version: number }>).find((item) => item.content.title === '快捷记录自动保存');
    expect(saved?.version).toBe(1);
  });

  it('adds the built-in account marker as ordinary record metadata', async () => {
    history.replaceState(null, '', '/?capture=text');
    render(<App />);
    await screen.findByPlaceholderText('记录标题');
    fireEvent.click(screen.getByRole('button', { name: '特殊标记' }));
    fireEvent.click(screen.getByRole('button', { name: '账目' }));
    await waitFor(() => {
      const items = JSON.parse(localStorage.getItem('snapline-dev-items-v1') || '[]') as Array<{ content: { markers: string[] } }>;
      expect(items.some((item) => item.content.markers.includes('账目'))).toBe(true);
    });
    expect(screen.queryByText('收入')).not.toBeInTheDocument();
    expect(screen.queryByText('支出')).not.toBeInTheDocument();
  });

  it('creates a custom special marker in quick capture and automatically saves it', async () => {
    history.replaceState(null, '', '/?capture=text');
    render(<App />);
    await screen.findByPlaceholderText('记录标题');
    fireEvent.click(screen.getByRole('button', { name: '特殊标记' }));
    const input = screen.getByPlaceholderText('新建特殊标记');
    fireEvent.change(input, { target: { value: '  等待   回复  ' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => {
      const items = JSON.parse(localStorage.getItem('snapline-dev-items-v1') || '[]') as Array<{ content: { markers: string[] } }>;
      expect(items.some((item) => item.content.markers.includes('等待 回复'))).toBe(true);
    });
  });

  it('opens searchable history and jumps to the selected record', async () => {
    render(<App />);
    await screen.findByText('关于新产品首页的想法');
    fireEvent.click(screen.getByRole('button', { name: '历史记录' }));
    expect(screen.getByRole('dialog', { name: '历史记录' })).toBeInTheDocument();
    fireEvent.change(screen.getByPlaceholderText('搜索历史记录'), { target: { value: '语音' } });
    fireEvent.click(within(screen.getByRole('dialog', { name: '历史记录' })).getByRole('button', { name: /散步时的语音想法/ }));
    expect(screen.queryByRole('dialog', { name: '历史记录' })).not.toBeInTheDocument();
    expect(await screen.findByDisplayValue('散步时的语音想法')).toBeInTheDocument();
  });

  it('archives and restores a record from the timeline action menu', async () => {
    render(<App />);
    await screen.findByText('团队午餐票据');
    fireEvent.click(screen.getByRole('button', { name: '更多操作：团队午餐票据' }));
    fireEvent.click(screen.getByRole('button', { name: '归档：团队午餐票据' }));
    await waitFor(() => {
      const items = JSON.parse(localStorage.getItem('snapline-dev-items-v1') || '[]') as Array<{ content: { title: string }; archived: boolean }>;
      expect(items.find((item) => item.content.title === '团队午餐票据')?.archived).toBe(true);
    });
    fireEvent.click(within(screen.getByRole('navigation')).getByRole('button', { name: '归档' }));
    expect(await screen.findByText('团队午餐票据')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '更多操作：团队午餐票据' }));
    fireEvent.click(screen.getByRole('button', { name: '恢复：团队午餐票据' }));
    await waitFor(() => {
      const items = JSON.parse(localStorage.getItem('snapline-dev-items-v1') || '[]') as Array<{ content: { title: string }; archived: boolean }>;
      expect(items.find((item) => item.content.title === '团队午餐票据')?.archived).toBe(false);
    });
  });

  it('adds and removes a record from favorites', async () => {
    render(<App />);
    await screen.findByText('团队午餐票据');
    fireEvent.click(screen.getByRole('button', { name: '更多操作：团队午餐票据' }));
    fireEvent.click(screen.getByRole('button', { name: '收藏：团队午餐票据' }));
    fireEvent.click(within(screen.getByRole('navigation')).getByRole('button', { name: '收藏' }));
    expect(screen.getByText('团队午餐票据')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '更多操作：团队午餐票据' }));
    fireEvent.click(screen.getByRole('button', { name: '取消收藏：团队午餐票据' }));
    await waitFor(() => expect(screen.queryByText('团队午餐票据')).not.toBeInTheDocument());
  });

  it('combines source, marker, and tag filters and can clear them', async () => {
    render(<App />);
    await screen.findByText('团队午餐票据');
    fireEvent.click(screen.getByRole('button', { name: '筛选' }));
    fireEvent.click(screen.getByRole('checkbox', { name: '图片' }));
    fireEvent.click(screen.getByRole('checkbox', { name: '账目' }));
    fireEvent.click(screen.getByRole('checkbox', { name: '#项目' }));
    expect(screen.getByText('团队午餐票据')).toBeInTheDocument();
    expect(screen.queryByText('关于新产品首页的想法')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('checkbox', { name: '重要' }));
    expect(screen.getByText('没有找到记录')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '清除' }));
    expect(screen.getByText('团队午餐票据')).toBeInTheDocument();
    expect(screen.getByText('关于新产品首页的想法')).toBeInTheDocument();
  });
});
