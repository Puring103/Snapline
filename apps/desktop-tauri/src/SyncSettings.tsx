import { FormEvent, useState } from "react";
import { api } from "./api";
import type { SyncAccountState } from "./types";

interface SyncSettingsProps {
  initial: SyncAccountState | null;
  onSaved: (state: SyncAccountState) => void;
}

export function SyncSettings({ initial, onSaved }: SyncSettingsProps) {
  const [serverUrl, setServerUrl] = useState(initial?.server_base_url ?? "http://localhost:8080");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [status, setStatus] = useState(initial?.is_logged_in ? "Connected" : "Not connected");

  async function submit(event: FormEvent) {
    event.preventDefault();
    setStatus("Connecting");
    try {
      const next = await api.loginSync(serverUrl, email, password);
      onSaved(next);
      setStatus("Connected");
    } catch (err) {
      setStatus(String(err));
    }
  }

  return (
    <form className="syncSettings" onSubmit={submit}>
      <input
        aria-label="Sync server URL"
        onChange={(event) => setServerUrl(event.target.value)}
        value={serverUrl}
      />
      <input aria-label="Email" onChange={(event) => setEmail(event.target.value)} value={email} />
      <input
        aria-label="Password"
        onChange={(event) => setPassword(event.target.value)}
        type="password"
        value={password}
      />
      <button type="submit">Connect</button>
      <span className="syncSettingsStatus">{status}</span>
    </form>
  );
}
