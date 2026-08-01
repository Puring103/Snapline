import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { AiPanel } from './AiPanel';
import { askAgent } from '../lib/native';

vi.mock('../lib/native', () => ({ askAgent: vi.fn() }));

const mockedAskAgent = vi.mocked(askAgent);

beforeEach(() => mockedAskAgent.mockReset());

it('asks the agent, renders citations, and opens the cited record', async () => {
  const onSelectCitation = vi.fn();
  mockedAskAgent.mockResolvedValue({
    answer: '你在产品首页记录里提到了减少视觉噪音。',
    rounds: 2,
    citations: [{ id: 'record-1', title: '关于新产品首页的想法', summary: '减少视觉噪音', source_type: 'text', updated_at: '2026-01-01T00:00:00Z' }],
  });
  render(<AiPanel configured onConfigure={vi.fn()} onClose={vi.fn()} onSelectCitation={onSelectCitation} />);
  fireEvent.change(screen.getByPlaceholderText('从我的记录中寻找…'), { target: { value: '首页有哪些想法' } });
  fireEvent.click(screen.getByRole('button', { name: '发送' }));
  expect(await screen.findByText('你在产品首页记录里提到了减少视觉噪音。')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: '打开记录：关于新产品首页的想法' }));
  expect(onSelectCitation).toHaveBeenCalledWith('record-1');
});

it('blocks oversized input before it reaches the agent', () => {
  render(<AiPanel configured onConfigure={vi.fn()} onClose={vi.fn()} onSelectCitation={vi.fn()} />);
  fireEvent.change(screen.getByPlaceholderText('从我的记录中寻找…'), { target: { value: '问'.repeat(4_001) } });
  expect(screen.getByText('最多输入 4000 个字符')).toBeInTheDocument();
  expect(screen.getByRole('button', { name: '发送' })).toBeDisabled();
  expect(mockedAskAgent).not.toHaveBeenCalled();
});

it('keeps previous messages when a later request fails', async () => {
  mockedAskAgent.mockResolvedValueOnce({ answer: '第一次回答', rounds: 1, citations: [] }).mockRejectedValueOnce(new Error('模型暂时不可用'));
  render(<AiPanel configured onConfigure={vi.fn()} onClose={vi.fn()} onSelectCitation={vi.fn()} />);
  const input = screen.getByPlaceholderText('从我的记录中寻找…');
  fireEvent.change(input, { target: { value: '第一次' } });
  fireEvent.click(screen.getByRole('button', { name: '发送' }));
  expect(await screen.findByText('第一次回答')).toBeInTheDocument();
  fireEvent.change(input, { target: { value: '第二次' } });
  fireEvent.click(screen.getByRole('button', { name: '发送' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('模型暂时不可用');
  expect(screen.getByText('第一次回答')).toBeInTheDocument();
  await waitFor(() => expect(screen.getByRole('button', { name: '发送' })).toBeDisabled());
});

it('shows configuration entry and disables the composer without AI settings', () => {
  const onConfigure = vi.fn();
  render(<AiPanel configured={false} onConfigure={onConfigure} onClose={vi.fn()} onSelectCitation={vi.fn()} />);
  fireEvent.click(screen.getByRole('button', { name: '配置模型' }));
  expect(onConfigure).toHaveBeenCalledOnce();
  expect(screen.getByPlaceholderText('配置 AI 后即可搜索')).toBeDisabled();
});
