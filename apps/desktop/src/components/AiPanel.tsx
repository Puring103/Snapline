import { useState } from 'react';
import { ArrowUp, Bot, KeyRound, Sparkles, X } from 'lucide-react';

export function AiPanel({ configured, onConfigure, onClose }: { configured: boolean; onConfigure: () => void; onClose: () => void }) {
  const [query, setQuery] = useState('');
  return <section className="ai-panel">
    <header><div><span className="ai-icon"><Sparkles size={16} /></span><div><strong>问问 Snapline</strong><small>{configured ? 'Agent 搜索已就绪' : '尚未配置 AI'}</small></div></div><button className="icon-button" onClick={onClose} aria-label="关闭 AI"><X size={18} /></button></header>
    {!configured ? <div className="ai-unconfigured"><span><KeyRound size={22} /></span><h2>连接你的 AI 模型</h2><p>配置一个兼容 OpenAI 格式的多模态模型，用来理解记录并进行 Agent 式搜索。</p><button className="primary-button" onClick={onConfigure}>配置模型</button><small>API Key 只保存在系统凭据库</small></div> : <div className="ai-thread"><div className="assistant-message"><Bot size={17} /><p>历史记录元数据与本地全文索引已就绪。Agent 对话将在下一模块接入受控搜索工具。</p></div></div>}
    <form className="ai-composer" onSubmit={(event) => event.preventDefault()}><textarea value={query} onChange={(event) => setQuery(event.target.value)} placeholder={configured ? '从我的记录中寻找…' : '配置 AI 后即可搜索'} disabled={!configured} /><button className="send-button" disabled={!configured || !query.trim()} aria-label="发送"><ArrowUp size={17} /></button></form>
  </section>;
}
