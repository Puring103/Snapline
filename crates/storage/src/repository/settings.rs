use anyhow::Result;
use rusqlite::params;

use super::NoteRepository;

impl NoteRepository {
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM settings WHERE key = ?1")?;
        let value = stmt.query_row(params![key], |row| row.get::<_, String>(0));
        match value {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    pub fn set_setting(&self, key: &str, value: Option<&str>) -> Result<()> {
        match value {
            Some(value) => {
                self.conn.execute(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![key, value],
                )?;
            }
            None => {
                self.conn
                    .execute("DELETE FROM settings WHERE key = ?1", params![key])?;
            }
        }
        Ok(())
    }
}
