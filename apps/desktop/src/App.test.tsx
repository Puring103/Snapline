import { fireEvent, render, screen, waitFor } from '@testing-library/react';
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
    const title = screen.getByPlaceholderText('无标题记录');
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
});
