use rusqlite::{Connection, params};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS clothing (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    type        TEXT NOT NULL CHECK(type IN ('top','bottom','dress','hat','accessory')),
    color_hex   TEXT DEFAULT '#FFFFFF',
    opacity     REAL DEFAULT 1.0,
    pattern     TEXT DEFAULT '',
    model_file  TEXT DEFAULT '',
    notes       TEXT DEFAULT '',
    created_at  TEXT DEFAULT (datetime('now')),
    is_active   INTEGER DEFAULT 0
);
"#;

pub struct ClothingDb {
    conn: Connection,
}

#[derive(Debug)]
pub struct ClothingItem {
    pub id: i64,
    pub name: String,
    pub clothing_type: String,
    pub color_hex: String,
    pub opacity: f64,
    pub model_file: String,
    pub notes: String,
    pub is_active: bool,
}

impl ClothingDb {
    pub fn open() -> anyhow::Result<Self> {
        let dir = dirs_db_path();
        std::fs::create_dir_all(&dir)?;
        let db_path = format!("{}/clothing.db", dir);
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(SCHEMA)?;
        log::info!("Clothing DB opened: {}", db_path);
        Ok(Self { conn })
    }

    pub fn list_all(&self) -> anyhow::Result<Vec<ClothingItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, type, color_hex, opacity, model_file, notes, is_active FROM clothing ORDER BY id"
        )?;
        let items = stmt.query_map([], |row| {
            Ok(ClothingItem {
                id: row.get(0)?,
                name: row.get(1)?,
                clothing_type: row.get(2)?,
                color_hex: row.get(3)?,
                opacity: row.get(4)?,
                model_file: row.get(5)?,
                notes: row.get(6)?,
                is_active: row.get::<_, i32>(7)? != 0,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(items)
    }

    pub fn list_active(&self) -> anyhow::Result<Vec<ClothingItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, type, color_hex, opacity, model_file, notes, is_active FROM clothing WHERE is_active = 1 ORDER BY id"
        )?;
        let items = stmt.query_map([], |row| {
            Ok(ClothingItem {
                id: row.get(0)?,
                name: row.get(1)?,
                clothing_type: row.get(2)?,
                color_hex: row.get(3)?,
                opacity: row.get(4)?,
                model_file: row.get(5)?,
                notes: row.get(6)?,
                is_active: row.get::<_, i32>(7)? != 0,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(items)
    }

    pub fn add(&self, name: &str, clothing_type: &str, color_hex: &str, model_file: &str) -> anyhow::Result<i64> {
        self.conn.execute(
            "INSERT INTO clothing (name, type, color_hex, model_file) VALUES (?1, ?2, ?3, ?4)",
            params![name, clothing_type, color_hex, model_file],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn delete(&self, id: i64) -> anyhow::Result<()> {
        self.conn.execute("DELETE FROM clothing WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn toggle_active(&self, id: i64) -> anyhow::Result<()> {
        self.conn.execute(
            "UPDATE clothing SET is_active = CASE WHEN is_active = 0 THEN 1 ELSE 0 END WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }
}

fn dirs_db_path() -> String {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        format!("{}/Library/Application Support/rPlayer", home)
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        format!("{}\\rPlayer", appdata)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        format!("{}/.local/share/rplayer", home)
    }
}
