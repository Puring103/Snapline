import { api } from "../../platform/api";
import { SyncSettings } from "../SyncSettings";
import { syncStatusLabel } from "../../features/sync/syncStatus";
import type { LoginSyncResult, SyncAccountState } from "../../types";
import { CloseIcon, IconButton, ThemeDarkIcon, ThemeLightIcon, ThemeSystemIcon } from "./AppIcons";
import type { ThemeMode } from "../../hooks/theme";

const DEFAULT_SHORTCUT = "Ctrl+Shift+Space";

interface SettingsPanelProps {
  onClose: () => void;
  shortcut: string;
  onShortcutChange: (value: string) => void;
  onShortcutSave: (value: string) => void;
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
  onShortcutChange,
  onShortcutSave,
  autostartEnabled,
  onAutostartChange,
  themeMode,
  onThemeModeChange,
  syncAccount,
  onSyncSaved,
}: SettingsPanelProps) {
  return (
    <div className="settingsBackdrop" onClick={onClose}>
      <section className="settingsPanel" onClick={(event) => event.stopPropagation()}>
        <header className="settingsHeader">
          <div>
            <div className="settingsTitle">Settings</div>
            <div className="settingsSub">Shortcut and appearance</div>
          </div>
          <div className="settingsHeaderActions">
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
            <IconButton label="Close settings" onClick={onClose}><CloseIcon /></IconButton>
          </div>
        </header>

        <div className="settingsPanelScroll">
          <div className="settingsGroup">
            <div className="settingsGroupTitle">General</div>
            <label className="settingsField">
              <span>Open shortcut</span>
              <div className="shortcutRow">
                <input
                  className="shortcutInput"
                  value={shortcut}
                  onChange={(event) => onShortcutChange(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      event.preventDefault();
                      onShortcutSave(shortcut);
                    }
                  }}
                  placeholder={DEFAULT_SHORTCUT}
                />
                <button className="drawerAction" onClick={() => onShortcutSave(shortcut)} type="button">
                  Save
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
            <div className="settingsGroupTitle">Sync</div>
            <div className="settingsField">
              <span>Status</span>
              <div className="settingsSyncStatus">{syncStatusLabel(syncAccount)}</div>
              <SyncSettings
                initial={syncAccount}
                onSaved={onSyncSaved}
                onSyncNow={async () => {
                  const report = await api.syncNow();
                  return report;
                }}
              />
            </div>
          </div>
        </div>
      </section>
    </div>
  );
}
