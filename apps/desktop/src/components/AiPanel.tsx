import { FormEvent, KeyboardEvent, useState } from 'react';
import { ArrowRight, ArrowUp, Bot, KeyRound, LoaderCircle, Sparkles, X } from 'lucide-react';
import { askAgent, type AgentCitation } from '../lib/native';

type Message = { id: string; role: 'user' | 'assistant'; text: string; citations?: AgentCitation[] };

export function AiPanel({ configured, onConfigure, onClose, onSelectCitation }: { configured: boolean; onConfigure: () => void; onClose: () => void; onSelectCitation: (id: string) => void }) {
  const [query, setQuery] = useState('');
  const [messages, setMessages] = useState<Message[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const tooLong = [...query].length > 4_000;

  const submit = async (event?: FormEvent) => {
    event?.preventDefault();
    const question = query.trim();
    if (!configured || loading || !question || tooLong) return;
    const userMessage: Message = { id: crypto.randomUUID(), role: 'user', text: question };
    setMessages((current) => [...current, userMessage]);
    setQuery('');
    setError('');
    setLoading(true);
    try {
      const result = await askAgent(question);
      setMessages((current) => [...current, { id: crypto.randomUUID(), role: 'assistant', text: result.answer, citations: result.citations }]);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Agent 搜索失败，请稍后重试');
    } finally {
      setLoading(false);
    }
  };
  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); void submit(); }
  };
  return <section className="ai-panel">
    <header><div><span className="ai-icon"><Sparkles size={16} /></span><div><strong>问问 Snapline</strong><small>{configured ? 'Agent 搜索已就绪' : '尚未配置 AI'}</small></div></div><button className="icon-button" onClick={onClose} aria-label="关闭 AI"><X size={18} /></button></header>
    {!configured ? <div className="ai-unconfigured"><span><KeyRound size={22} /></span><h2>连接你的 AI 模型</h2><p>配置一个兼容 OpenAI 格式的多模态模型，用来理解记录并进行 Agent 式搜索。</p><button className="primary-button" onClick={onConfigure}>配置模型</button><small>API Key 只保存在系统凭据库</small></div> : <div className="ai-thread" aria-live="polite">
      {!messages.length && <div className="assistant-message"><Bot size={17} /><p>可以按主题、时间、标签或特殊标记查找历史记录。回答只引用本次搜索实际读取的内容。</p></div>}
      {messages.map((message) => <article key={message.id} className={`ai-message ${message.role}`}><div>{message.role === 'assistant' && <Bot size={15} />}<p>{message.text}</p></div>{message.citations?.map((citation) => <button key={citation.id} className="ai-citation" onClick={() => onSelectCitation(citation.id)} aria-label={`打开记录：${citation.title}`}><span><strong>{citation.title}</strong><small>{citation.summary || citation.source_type}</small></span><ArrowRight size={14} /></button>)}</article>)}
      {loading && <div className="ai-loading"><LoaderCircle size={15} /><span>正在搜索记录…</span></div>}
      {error && <div className="ai-error" role="alert">{error}</div>}
    </div>}
    <form className="ai-composer" onSubmit={(event) => void submit(event)}><textarea maxLength={4_001} value={query} onKeyDown={onKeyDown} onChange={(event) => setQuery(event.target.value)} placeholder={configured ? '从我的记录中寻找…' : '配置 AI 后即可搜索'} disabled={!configured || loading} aria-invalid={tooLong} /><button className="send-button" disabled={!configured || loading || !query.trim() || tooLong} aria-label="发送"><ArrowUp size={17} /></button>{tooLong && <small className="ai-input-error">最多输入 4000 个字符</small>}</form>
  </section>;
}
