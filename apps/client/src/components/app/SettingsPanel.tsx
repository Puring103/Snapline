import { useEffect, useRef, useState } from "react";
import { SyncSettings } from "../SyncSettings";
import { syncStatusLabel } from "../../features/sync/syncStatus";
import type { LoginSyncResult, SyncAccountState } from "../../types";
import { CloseIcon, IconButton, ThemeDarkIcon, ThemeLightIcon, ThemeSystemIcon } from "./AppIcons";
import type { ThemeMode } from "../../hooks/theme";

interface SettingsPanelProps {
  onClose: () => void;
  shortcut: string;
  onShortcutSave: (value: string) => Promise<boolean>;
  autostartEnabled: boolean;
  onAutostartChange: (value: boolean) => void;
  themeMode: ThemeMode;
  onThemeModeChange: (value: ThemeMode) => void;
  syncAccount: SyncAccountState | null;
  onSyncSaved: (result: LoginSyncResult) => void;
}

export function SettingsPanel({
  onClose,
  shortcut,
  onShortcutSave,
  autostartEnabled,
  onAutostartChange,
  themeMode,
  onThemeModeChange,
  syncAccount,
  onSyncSaved,
}: SettingsPanelProps) {
  const [listeningShortcut, setListeningShortcut] = useState(false);
  const [pendingShortcut, setPendingShortcut] = useState(shortcut);
  const [failedShortcut, setFailedShortcut] = useState("");
  const [shortcutSaveFailed, setShortcutSaveFailed] = useState(false);
  const shortcutTimerRef = useRef<number | null>(null);
  const shortcutErrorTimerRef = useRef<number | null>(null);

  useEffect(() => {
    if (!listeningShortcut) {
      setPendingShortcut(shortcut);
    }
  }, [listeningShortcut, shortcut]);

  useEffect(() => {
    return () => {
      if (shortcutTimerRef.current !== null) {
        window.clearTimeout(shortcutTimerRef.current);
      }
      if (shortcutErrorTimerRef.current !== null) {
        window.clearTimeout(shortcutErrorTimerRef.current);
      }
    };
  }, []);

  function completeShortcutCapture(nextShortcut: string) {
    setPendingShortcut(nextShortcut);
    setShortcutSaveFailed(false);
    if (shortcutTimerRef.current !== null) {
      window.clearTimeout(shortcutTimerRef.current);
    }
    shortcutTimerRef.current = window.setTimeout(() => {
      setListeningShortcut(false);
      void saveShortcut(nextShortcut);
    }, 650);
  }

  async function saveShortcut(nextShortcut: string) {
    const saved = await onShortcutSave(nextShortcut);
    if (saved) {
      setShortcutSaveFailed(false);
      setFailedShortcut("");
      return;
    }

    setFailedShortcut(nextShortcut);
    setShortcutSaveFailed(true);
    if (shortcutErrorTimerRef.current !== null) {
      window.clearTimeout(shortcutErrorTimerRef.current);
    }
    shortcutErrorTimerRef.current = window.setTimeout(() => {
      setPendingShortcut(shortcut);
      setFailedShortcut("");
      setShortcutSaveFailed(false);
    }, 1200);
  }

  function shortcutFromKeyboardEvent(event: React.KeyboardEvent<HTMLButtonElement>) {
    const parts = [
      event.ctrlKey ? "Ctrl" : "",
      event.metaKey ? "Meta" : "",
      event.altKey ? "Alt" : "",
      event.shiftKey ? "Shift" : "",
    ].filter(Boolean);
    const key = normalizedShortcutKey(event.key);
    return key ? [...parts, key].join("+") : parts.join("+");
  }

  return (
    <div className="settingsBackdrop" onClick={onClose}>
      <section className="settingsPanel" onClick={(event) => event.stopPropagation()}>
        <header className="settingsHeader">
          <div>
            <div className="settingsTitle">Settings</div>
            <div className="settingsSub">Shortcut and appearance</div>
          </div>
          <IconButton label="Close settings" onClick={onClose}><CloseIcon /></IconButton>
        </header>

        <div className="settingsPanelScroll">
          <div className="settingsGroup">
            <div className="settingsGroupTitle">登录</div>
            <div className="settingsField">
              <span>账号登录</span>
              <div className="settingsSyncStatus">{syncStatusLabel(syncAccount)}</div>
              <SyncSettings
                initial={syncAccount}
                onSaved={onSyncSaved}
              />
            </div>
          </div>

          <div className="settingsGroup">
            <div className="settingsGroupTitle">General</div>
            <label className="settingsField">
              <span>Open shortcut</span>
              <div className="shortcutRow">
                <div
                  className={[
                    "shortcutDisplay",
                    listeningShortcut ? "listening" : "",
                    shortcutSaveFailed ? "failed" : "",
                  ].filter(Boolean).join(" ")}
                >
                  {listeningShortcut ? pendingShortcut || "Press shortcut" : shortcutSaveFailed ? failedShortcut : shortcut}
                </div>
                <button
                  className="drawerAction"
                  onClick={() => {
                    setPendingShortcut("");
                    setListeningShortcut(true);
                  }}
                  onKeyDown={(event) => {
                    if (!listeningShortcut) return;
                    if (event.key === "Escape") {
                      event.preventDefault();
                      setListeningShortcut(false);
                      setPendingShortcut(shortcut);
                      return;
                    }
                    event.preventDefault();
                    const nextShortcut = shortcutFromKeyboardEvent(event);
                    if (nextShortcut) completeShortcutCapture(nextShortcut);
                  }}
                  type="button"
                >
                  Change
                </button>
              </div>
            </label>

            <label className="settingsToggle">
              <span>Start at login</span>
              <input
                checked={autostartEnabled}
                onChange={(event) => onAutostartChange(event.target.checked)}
                type="checkbox"
              />
            </label>
          </div>

          <div className="settingsGroup">
            <div className="settingsGroupTitle">Appearance</div>
            <div className="settingsField">
              <span>Theme</span>
              <div className="themeButtons">
                <IconButton
                  active={themeMode === "system"}
                  label="System theme"
                  onClick={() => onThemeModeChange("system")}
                >
                  <ThemeSystemIcon />
                </IconButton>
                <IconButton
                  active={themeMode === "light"}
                  label="Light theme"
                  onClick={() => onThemeModeChange("light")}
                >
                  <ThemeLightIcon />
                </IconButton>
                <IconButton
                  active={themeMode === "dark"}
                  label="Dark theme"
                  onClick={() => onThemeModeChange("dark")}
                >
                  <ThemeDarkIcon />
                </IconButton>
              </div>
            </div>
          </div>

        </div>
      </section>
    </div>
  );
}

function normalizedShortcutKey(key: string) {
  if (key === "Control" || key === "Shift" || key === "Alt" || key === "Meta") {
    return "";
  }
  if (key === " ") {
    return "Space";
  }
  if (key.length === 1) {
    return key.toUpperCase();
  }
  return key;
}
