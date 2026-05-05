use anyhow::Result;

use crate::AppCore;

const OPEN_SHORTCUT_KEY: &str = "open_shortcut";
const DEFAULT_OPEN_SHORTCUT: &str = "Ctrl+Shift+Space";

impl AppCore {
    pub fn get_open_shortcut(&self) -> Result<String> {
        Ok(self
            .repo
            .get_setting(OPEN_SHORTCUT_KEY)?
            .unwrap_or_else(|| DEFAULT_OPEN_SHORTCUT.to_string()))
    }

    pub fn set_open_shortcut(&self, shortcut: &str) -> Result<()> {
        self.repo.set_setting(OPEN_SHORTCUT_KEY, Some(shortcut))
    }
}
