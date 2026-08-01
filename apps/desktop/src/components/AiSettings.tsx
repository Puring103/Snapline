import { useEffect, useState } from 'react';
import { KeyRound, RefreshCw, ShieldCheck, Trash2, X } from 'lucide-react';
import { clearAiConfig, processAiQueue, rebuildAiMetadata, setAiConfig, type AiConfigStatus } from '../lib/native';

export function AiSettings({ status, onChange, onClose }: { status: AiConfigStatus; onChange: (status: AiConfigStatus) => void; onClose: () => void }) {
  const [baseUrl, setBaseUrl] = useState(status.base_url || 'https://api.openai.com/v1');
  const [model, setModel] = useState(status.model || '');
  const [apiKey, setApiKey] = useState('');
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');

  useEffect(() => {
    setBaseUrl(status.base_url || 'https://api.openai.com/v1');
    setModel(status.model || '');
  }, [status.base_url, status.model]);

  async function save(event: React.FormEvent) {
    event.preventDefault();
    setBusy(true); setError(''); setMessage('正在验证模型能力…');
    try {
      const next = await setAiConfig(baseUrl, model, apiKey);
      onChange(next); setApiKey(''); setMessage('配置已保存，正在构建历史元数据…');
      const result = await processAiQueue();
      setMessage(`本轮完成 ${result.completed} 条，失败 ${result.failed} 条`);
    } catch (reason) {
      setMessage(''); setError(reason instanceof Error ? reason.message : String(reason));
    } finally { setBusy(false); }
  }

  async function rebuild() {
    setBusy(true); setError(''); setMessage('正在重建处理队列…');
    try {
      const queued = await rebuildAiMetadata();
      const result = await processAiQueue();
      setMessage(`已排队 ${queued} 条，本轮完成 ${result.completed} 条，失败 ${result.failed} 条`);
    } catch (reason) { setMessage(''); setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setBusy(false); }
  }

  async function remove() {
    setBusy(true); setError('');
    try {
      await clearAiConfig();
      onChange({ configured: false, base_url: null, model: null, processing: false });
      setApiKey(''); setMessage('AI 配置已移除，本地记录不受影响');
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setBusy(false); }
  }

  return <div className="settings-backdrop" role="presentation">
    <section className="settings-dialog" role="dialog" aria-modal="true" aria-label="AI 模型设置">
      <header><div><span><KeyRound size={17} /></span><div><strong>AI 模型</strong><small>OpenAI 兼容格式 · 单一多模态模型</small></div></div><button className="icon-button" aria-label="关闭 AI 设置" onClick={onClose}><X size={18} /></button></header>
      <form onSubmit={(event) => void save(event)}>
        <label>API Base URL<input type="url" required value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} placeholder="https://api.example.com/v1" /></label>
        <label>模型名称<input required value={model} onChange={(event) => setModel(event.target.value)} placeholder="输入支持多模态和 JSON Schema 的模型" /></label>
        <label>API Key<input type="password" required={!status.configured} value={apiKey} onChange={(event) => setApiKey(event.target.value)} placeholder={status.configured ? '已安全保存，留空表示不修改' : '仅保存到 Windows 凭据管理器'} /></label>
        <div className="settings-security"><ShieldCheck size={15} /><span>Key 只在这台设备上调用模型，不会上传到 Snapline 服务端。</span></div>
        {error && <div className="settings-error" role="alert">{error}</div>}
        {message && <div className="settings-message" role="status">{message}</div>}
        <div className="settings-actions"><button type="submit" className="primary-button" disabled={busy}>{busy ? '处理中…' : '验证并保存'}</button>{status.configured && <button type="button" className="secondary-button" disabled={busy} onClick={() => void rebuild()}><RefreshCw size={14} />重建历史元数据</button>}</div>
      </form>
      {status.configured && <footer><button disabled={busy} onClick={() => void remove()}><Trash2 size={14} />移除 AI 配置</button></footer>}
    </section>
  </div>;
}
