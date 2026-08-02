//! SQLite layer: rusqlite (bundled), WAL mode, additive schema migration.
//! All access is serialized behind one Mutex'd connection, called via
//! `tokio::task::spawn_blocking` through `Db::call`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use ninty_core::error::{Error, Result};
use rusqlite::Connection;

const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS _meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  data TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS provider_connections (
  id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  auth_type TEXT NOT NULL,
  name TEXT,
  email TEXT,
  priority INTEGER NOT NULL DEFAULT 0,
  is_active INTEGER NOT NULL DEFAULT 1,
  data TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS provider_nodes (
  id TEXT PRIMARY KEY,
  type TEXT NOT NULL,
  name TEXT,
  data TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS api_keys (
  id TEXT PRIMARY KEY,
  key TEXT NOT NULL UNIQUE,
  name TEXT,
  machine_id TEXT,
  is_active INTEGER NOT NULL DEFAULT 1,
  token_limit INTEGER,
  limit_window TEXT,
  rpm_limit INTEGER,
  allowed_models TEXT NOT NULL DEFAULT '[]',
  limit_reset_at TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS combos (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  kind TEXT,
  models TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS kv (
  scope TEXT NOT NULL,
  key TEXT NOT NULL,
  value TEXT NOT NULL,
  PRIMARY KEY (scope, key)
);

CREATE TABLE IF NOT EXISTS usage_history (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts TEXT NOT NULL,
  provider TEXT,
  model TEXT,
  connection_id TEXT,
  api_key TEXT,
  endpoint TEXT,
  prompt_tokens INTEGER NOT NULL DEFAULT 0,
  completion_tokens INTEGER NOT NULL DEFAULT 0,
  cost REAL NOT NULL DEFAULT 0,
  status TEXT,
  tokens TEXT,
  meta TEXT
);

CREATE TABLE IF NOT EXISTS usage_daily (
  date_key TEXT PRIMARY KEY,
  data TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS request_details (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts TEXT NOT NULL,
  provider TEXT,
  model TEXT,
  connection_id TEXT,
  status TEXT,
  data TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_usage_ts ON usage_history(ts);
CREATE INDEX IF NOT EXISTS idx_usage_provider ON usage_history(provider);
CREATE INDEX IF NOT EXISTS idx_req_details_ts ON request_details(ts);
"#;

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    /// Open (and create if missing) the database at `path`, run migrations.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Db(format!("create dir {}: {e}", parent.display())))?;
        }
        let conn = Connection::open(path).map_err(|e| Error::Db(format!("open: {e}")))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| Error::Db(format!("wal: {e}")))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| Error::Db(format!("fk: {e}")))?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    /// Open an in-memory database (tests).
    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(|e| Error::Db(format!("open: {e}")))?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.lock()?;
        let version: i64 = conn
            .query_row(
                "SELECT value FROM _meta WHERE key = 'schema_version'",
                [],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        if version < 1 {
            conn.execute_batch(SCHEMA)
                .map_err(|e| Error::Db(format!("migrate v1: {e}")))?;
            conn.execute(
                "INSERT OR REPLACE INTO _meta (key, value) VALUES ('schema_version', ?1)",
                [SCHEMA_VERSION.to_string()],
            )
            .map_err(|e| Error::Db(format!("meta: {e}")))?;
        }
        Ok(())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|e| Error::Db(format!("poisoned: {e}")))
    }

    /// Run a blocking closure against the connection on the blocking thread pool.
    pub async fn call<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let guard = conn
                .lock()
                .map_err(|e| Error::Db(format!("poisoned: {e}")))?;
            f(&guard)
        })
        .await
        .map_err(|e| Error::Db(format!("join: {e}")))?
    }

    /// Sync variant for non-async contexts (startup, tests).
    /// Flush WAL to the main db file (graceful shutdown).
    pub fn checkpoint(&self) -> Result<()> {
        self.call_sync(|conn| {
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                .map_err(|e| ninty_core::error::Error::Db(e.to_string()))
        })
    }

    pub fn call_sync<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T>,
    {
        let guard = self.lock()?;
        f(&guard)
    }
}
