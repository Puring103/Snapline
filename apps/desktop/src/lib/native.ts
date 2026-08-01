export const isTauri = () => '__TAURI_INTERNALS__' in window;

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const api = await import('@tauri-apps/api/core');
  return api.invoke<T>(command, args);
}

export interface AuthResult {
  user_id: string;
  device_id: string;
  recovery_key: string | null;
}

export interface SessionStatus {
  authenticated: boolean;
  user_id: string | null;
  device_id?: string | null;
  access_expires_at?: string | null;
}

export async function sessionStatus(): Promise<SessionStatus> {
  if (!isTauri()) {
    return {
      authenticated: localStorage.getItem('snapline-dev-session') === 'authenticated',
      user_id: null,
    };
  }
  return invoke<SessionStatus>('auth_status');
}

export async function authenticate(input: {
  email: string;
  password: string;
  deviceName: string;
  registering: boolean;
}): Promise<AuthResult> {
  if (!isTauri()) {
    localStorage.setItem('snapline-dev-session', 'authenticated');
    return { user_id: 'development-user', device_id: 'development-device', recovery_key: null };
  }
  return invoke<AuthResult>(input.registering ? 'register_account' : 'login_account', {
    email: input.email,
    password: input.password,
    deviceName: input.deviceName,
  });
}

export async function logout(): Promise<void> {
  if (!isTauri()) {
    localStorage.removeItem('snapline-dev-session');
    return;
  }
  await invoke<void>('logout_account');
}
