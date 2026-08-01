import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { AiSettings } from './AiSettings';

beforeEach(() => localStorage.clear());

it('stores only non-secret development configuration and never echoes the API key', async () => {
  const onChange = vi.fn();
  render(<AiSettings status={{ configured: false, base_url: null, model: null, processing: false }} onChange={onChange} onClose={() => undefined} />);
  fireEvent.change(screen.getByLabelText('API Base URL'), { target: { value: 'https://ai.example/v1' } });
  fireEvent.change(screen.getByLabelText('模型名称'), { target: { value: 'user-multimodal-model' } });
  fireEvent.change(screen.getByLabelText('API Key'), { target: { value: 'super-secret-key' } });
  fireEvent.click(screen.getByRole('button', { name: '验证并保存' }));
  await waitFor(() => expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ configured: true, model: 'user-multimodal-model' })));
  expect(localStorage.getItem('snapline-dev-ai-config')).not.toContain('super-secret-key');
  expect(screen.getByLabelText('API Key')).toHaveValue('');
});

it('allows an existing configuration to keep its credential field blank', async () => {
  localStorage.setItem('snapline-dev-ai-config', JSON.stringify({ base_url: 'https://ai.example/v1', model: 'old-model' }));
  const onChange = vi.fn();
  render(<AiSettings status={{ configured: true, base_url: 'https://ai.example/v1', model: 'old-model', processing: false }} onChange={onChange} onClose={() => undefined} />);
  fireEvent.change(screen.getByLabelText('模型名称'), { target: { value: 'new-model' } });
  fireEvent.click(screen.getByRole('button', { name: '验证并保存' }));
  await waitFor(() => expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ model: 'new-model' })));
});
