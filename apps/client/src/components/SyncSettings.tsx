import { FormEvent, useEffect, useState } from "react";
import { api } from "../platform/api";
import type { LoginSyncResult, SyncAccountState } from "../types";

interface SyncSettingsProps {
  initial: SyncAccountState | null;
  onSaved: (result: LoginSyncResult) => void;
}

interface SyncConnectionFields {
  serverUrl: string;
  email: string;
  password: string;
}

type SyncConnectionErrors = Partial<Record<keyof SyncConnectionFields, string>>;

export function validateSyncConnection(fields: SyncConnectionFields): SyncConnectionErrors {
  const errors: SyncConnectionErrors = {};
  const serverUrl = fields.serverUrl.trim();
  const email = fields.email.trim();

  if (!serverUrl) {
    errors.serverUrl = "Server URL is required";
  } else {
    try {
      const url = new URL(serverUrl);
      if (url.protocol !== "http:" && url.protocol !== "https:") {
        errors.serverUrl = "Enter a valid http or https URL";
      }
    } catch {
      errors.serverUrl = "Enter a valid http or https URL";
    }
  }

  if (!email) {
    errors.email = "Email is required";
  } else if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) {
    errors.email = "Enter a valid email address";
  }

  if (!fields.password) {
    errors.password = "Password is required";
  }

  return errors;
}

export function importPromptText(count: number): string {
  return `Detected ${count} local ${count === 1 ? "note" : "notes"}. Import into this account?`;
}

export function SyncSettings({ initial, onSaved }: SyncSettingsProps) {
  const [serverUrl, setServerUrl] = useState(initial?.server_base_url ?? "http://localhost:8080");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [mode, setMode] = useState<"login" | "register">("login");
  const [status, setStatus] = useState(initial?.is_logged_in ? "Connected" : "Not connected");
  const [dialogOpen, setDialogOpen] = useState(false);
  const [errors, setErrors] = useState<SyncConnectionErrors>({});

  useEffect(() => {
    setStatus(initial?.is_logged_in ? "Connected" : "Not connected");
    if (initial?.server_base_url) {
      setServerUrl(initial.server_base_url);
    }
  }, [initial?.is_logged_in, initial?.server_base_url]);

  async function submit(event: FormEvent) {
    event.preventDefault();
    const nextErrors = validateSyncConnection({ serverUrl, email, password });
    setErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0) {
      return;
    }

    setStatus(mode === "register" ? "Creating account" : "Connecting");
    try {
      const next = mode === "register"
        ? await api.registerSync(serverUrl.trim(), email.trim(), password)
        : await api.loginSync(serverUrl.trim(), email.trim(), password);
      onSaved(next);
      setStatus("Connected");
      setDialogOpen(false);
      setPassword("");
    } catch (err) {
      setStatus(String(err));
    }
  }

  function openDialog(nextMode: "login" | "register") {
    setMode(nextMode);
    setDialogOpen(true);
  }

  async function logout() {
    setStatus("Disconnecting");
    try {
      const next = await api.logoutSync();
      onSaved(next);
      setEmail("");
      setPassword("");
      setStatus("Not connected");
    } catch (err) {
      setStatus(String(err));
    }
  }

  return (
    <div className="syncSettings">
      <div className="syncSettingsSummary">
        <span className={status === "Connecting" || status === "Creating account" ? "syncSettingsStatus syncing" : "syncSettingsStatus"}>{status}</span>
        {initial?.server_base_url ? <span className="syncSettingsServer">{initial.server_base_url}</span> : null}
      </div>
      <div className="syncSettingsActions">
        {initial?.is_logged_in ? (
          <button onClick={() => void logout()} type="button">退出账号</button>
        ) : (
          <>
            <button onClick={() => openDialog("login")} type="button">登录</button>
            <button onClick={() => openDialog("register")} type="button">注册</button>
          </>
        )}
      </div>

      {dialogOpen ? (
        <div className="connectionDialogBackdrop" onClick={() => setDialogOpen(false)}>
          <form className="connectionDialog" onClick={(event) => event.stopPropagation()} onSubmit={submit}>
            <header className="connectionDialogHeader">
              <div>
                <div className="connectionDialogTitle">账号登录</div>
                <div className="connectionDialogSub">Connect Snapline to your sync server</div>
              </div>
              <button aria-label="Close remote connection" className="dialogIconButton" onClick={() => setDialogOpen(false)} type="button">
                <span aria-hidden="true">x</span>
              </button>
            </header>

            <div className="connectionModeToggle">
              <button
                className={mode === "login" ? "connectionModeActive" : ""}
                onClick={() => setMode("login")}
                type="button"
              >
                登录
              </button>
              <button
                className={mode === "register" ? "connectionModeActive" : ""}
                onClick={() => setMode("register")}
                type="button"
              >
                注册
              </button>
            </div>

            <label className="connectionField">
              <span>Server URL</span>
              <input
                aria-invalid={errors.serverUrl ? "true" : "false"}
                onChange={(event) => {
                  setServerUrl(event.target.value);
                  setErrors((current) => ({ ...current, serverUrl: undefined }));
                }}
                placeholder="https://sync.example.com"
                value={serverUrl}
              />
              {errors.serverUrl ? <span className="connectionFieldError">{errors.serverUrl}</span> : null}
            </label>

            <label className="connectionField">
              <span>Email</span>
              <input
                aria-invalid={errors.email ? "true" : "false"}
                onChange={(event) => {
                  setEmail(event.target.value);
                  setErrors((current) => ({ ...current, email: undefined }));
                }}
                placeholder="you@example.com"
                type="email"
                value={email}
              />
              {errors.email ? <span className="connectionFieldError">{errors.email}</span> : null}
            </label>

            <label className="connectionField">
              <span>Password</span>
              <input
                aria-invalid={errors.password ? "true" : "false"}
                onChange={(event) => {
                  setPassword(event.target.value);
                  setErrors((current) => ({ ...current, password: undefined }));
                }}
                type="password"
                value={password}
              />
              {errors.password ? <span className="connectionFieldError">{errors.password}</span> : null}
            </label>

            <div className="connectionDialogActions">
              <button onClick={() => setDialogOpen(false)} type="button">Cancel</button>
              <button type="submit">{mode === "register" ? "注册" : "登录"}</button>
            </div>
            <span className="syncSettingsStatus">{status}</span>
          </form>
        </div>
      ) : null}
    </div>
  );
}
