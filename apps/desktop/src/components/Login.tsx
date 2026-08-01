import { useState } from 'react';
import { ArrowRight, Check, Copy, Eye, EyeOff, LockKeyhole } from 'lucide-react';
import { authenticate } from '../lib/native';
import { Logo } from './Logo';

export function Login({ onAuthenticated }: { onAuthenticated: () => void }) {
  const [passwordVisible, setPasswordVisible] = useState(false);
  const [registering, setRegistering] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [recoveryKey, setRecoveryKey] = useState('');

  async function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError('');
    setLoading(true);
    const form = new FormData(event.currentTarget);
    try {
      const result = await authenticate({
        email: String(form.get('email')),
        password: String(form.get('password')),
        deviceName: String(form.get('deviceName') || '我的 Windows 电脑'),
        registering,
      });
      if (result.recovery_key) setRecoveryKey(result.recovery_key);
      else onAuthenticated();
    } catch (reason) {
      setError(typeof reason === 'string' ? reason : '登录失败，请稍后重试');
    } finally {
      setLoading(false);
    }
  }

  if (recoveryKey) return <main className="auth-shell recovery-shell"><section className="auth-pane recovery-pane"><Logo /><div className="auth-copy"><span className="auth-kicker"><Check size={14} /> 账号已创建</span><h1>保存恢复密钥</h1><p>忘记密码后只能使用它恢复加密记录。Snapline 不会再次显示这串密钥。</p></div><div className="recovery-key"><code>{recoveryKey}</code><button type="button" className="icon-button" title="复制恢复密钥" aria-label="复制恢复密钥" onClick={() => void navigator.clipboard.writeText(recoveryKey)}><Copy size={17} /></button></div><button className="primary-button auth-submit" onClick={onAuthenticated}>我已安全保存</button></section><aside className="auth-visual" aria-hidden="true" /></main>;

  return <main className="auth-shell">
    <section className="auth-pane">
      <Logo />
      <div className="auth-copy">
        <span className="auth-kicker"><LockKeyhole size={14} /> 端到端加密</span>
        <h1>{registering ? '创建你的记录空间' : '继续记录'}</h1>
        <p>{registering ? '密钥只在你的设备上生成，服务器无法读取记录。' : '登录并解锁这台设备上的加密记录。'}</p>
      </div>
      <form className="auth-form" onSubmit={submit}>
        <label>邮箱<input name="email" required type="email" placeholder="you@example.com" autoFocus /></label>
        <label>密码<span className="password-field"><input name="password" required minLength={10} type={passwordVisible ? 'text' : 'password'} placeholder="至少 10 个字符" /><button type="button" className="icon-button field-action" onClick={() => setPasswordVisible(!passwordVisible)} aria-label={passwordVisible ? '隐藏密码' : '显示密码'}>{passwordVisible ? <EyeOff size={17} /> : <Eye size={17} />}</button></span></label>
        {registering && <label>设备名称<input name="deviceName" required defaultValue="我的 Windows 电脑" /></label>}
        {error && <div className="auth-error" role="alert">{error}</div>}
        <button className="primary-button auth-submit" disabled={loading}>{loading ? '正在解锁…' : registering ? '创建并解锁' : '登录并解锁'}<ArrowRight size={17} /></button>
      </form>
      <button className="text-button auth-switch" onClick={() => setRegistering(!registering)}>{registering ? '已有账号？登录' : '没有账号？创建账号'}</button>
      <footer>API 服务 · http://122.51.119.75/snapline</footer>
    </section>
    <aside className="auth-visual" aria-hidden="true">
      <div className="visual-date">8月 1日，星期六</div>
      <div className="visual-note visual-note-main"><span>09:42 · 文本</span><strong>关于新产品首页的想法</strong><p>不要急着解释所有功能，先让用户感受到捕捉的速度。</p><div><i>产品</i><i>设计</i></div></div>
      <div className="visual-note visual-note-audio"><span>08:16 · 录音 02:18</span><strong>散步时的语音想法</strong><div className="wave">{Array.from({ length: 32 }, (_, i) => <b key={i} style={{ height: `${8 + ((i * 13) % 25)}px` }} />)}</div></div>
      <div className="visual-thread"><span /><p>四种来源，一条清晰的时间线</p></div>
    </aside>
  </main>;
}
