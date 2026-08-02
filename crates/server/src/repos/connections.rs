use chrono::Utc;
use ninty_core::error::{Error, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::Db;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub id: String,
    pub provider: String,
    pub auth_type: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub priority: i64,
    pub is_active: bool,
    pub data: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

impl Connection {
    pub fn api_key(&self) -> Option<&str> {
        self.data.get("apiKey").and_then(|v| v.as_str())
    }

    /// Mask secrets for API responses.
    pub fn sanitized(&self) -> Self {
        let mut c = self.clone();
        if let Some(obj) = c.data.as_object_mut() {
            for key in [
                "apiKey",
                "accessToken",
                "refreshToken",
                "idToken",
                "clientSecret",
            ] {
                if let Some(v) = obj.get_mut(key) {
                    if let Some(s) = v.as_str() {
                        let tail: String = s
                            .chars()
                            .rev()
                            .take(4)
                            .collect::<String>()
                            .chars()
                            .rev()
                            .collect();
                        *v = serde_json::Value::String(format!("****{tail}"));
                    }
                }
            }
        }
        c
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewConnection {
    pub provider: String,
    pub name: Option<String>,
    pub priority: Option<i64>,
    pub api_key: Option<String>,
    pub data: Option<serde_json::Value>,
}

const COLUMNS: &str =
    "id, provider, auth_type, name, email, priority, is_active, data, created_at, updated_at";

fn row_to_conn(row: &rusqlite::Row<'_>) -> rusqlite::Result<Connection> {
    let data: String = row.get("data")?;
    Ok(Connection {
        id: row.get("id")?,
        provider: row.get("provider")?,
        auth_type: row.get("auth_type")?,
        name: row.get("name")?,
        email: row.get("email")?,
        priority: row.get("priority")?,
        is_active: row.get::<_, i64>("is_active")? == 1,
        data: serde_json::from_str(&data).unwrap_or(serde_json::json!({})),
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

pub async fn list(db: &Db, provider: Option<&str>) -> Result<Vec<Connection>> {
    let provider = provider.map(|s| s.to_string());
    db.call(move |conn| {
        let (sql, params): (String, Vec<String>) = match &provider {
            Some(p) => (
                format!("SELECT {COLUMNS} FROM provider_connections WHERE provider = ?1 ORDER BY priority ASC, created_at ASC"),
                vec![p.clone()],
            ),
            None => (
                format!("SELECT {COLUMNS} FROM provider_connections ORDER BY provider, priority ASC, created_at ASC"),
                vec![],
            ),
        };
        let mut stmt = conn.prepare(&sql).map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(params), row_to_conn)
            .map_err(|e| Error::Db(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Db(e.to_string()))?;
        Ok(rows)
    })
    .await
}

pub async fn create(db: &Db, new: NewConnection) -> Result<Connection> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let mut data = new.data.unwrap_or(serde_json::json!({}));
    if let Some(key) = &new.api_key {
        data["apiKey"] = serde_json::Value::String(key.clone());
    }
    let auth_type = if new.api_key.is_some() {
        "apikey"
    } else {
        "oauth"
    }
    .to_string();
    let data_str = serde_json::to_string(&data)?;
    let priority = new.priority.unwrap_or(0);
    let provider = new.provider.clone();
    let name = new.name.clone();
    db.call(move |conn| {
        conn.execute(
            "INSERT INTO provider_connections (id, provider, auth_type, name, priority, is_active, data, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7, ?7)",
            rusqlite::params![id, provider, auth_type, name, priority, data_str, now],
        )
        .map_err(|e| Error::Db(e.to_string()))?;
        Ok(Connection {
            id,
            provider,
            auth_type,
            name,
            email: None,
            priority,
            is_active: true,
            data,
            created_at: now.clone(),
            updated_at: now,
        })
    })
    .await
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ConnectionPatch {
    pub name: Option<String>,
    pub priority: Option<i64>,
    pub is_active: Option<bool>,
    pub api_key: Option<String>,
    pub data: Option<serde_json::Value>,
}

pub async fn update(db: &Db, id: &str, patch: ConnectionPatch) -> Result<()> {
    let id_owned = id.to_string();
    db.call(move |conn| {
        let now = Utc::now().to_rfc3339();
        if let Some(name) = &patch.name {
            conn.execute(
                "UPDATE provider_connections SET name = ?2, updated_at = ?3 WHERE id = ?1",
                rusqlite::params![id_owned, name, now],
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        }
        if let Some(p) = patch.priority {
            conn.execute(
                "UPDATE provider_connections SET priority = ?2, updated_at = ?3 WHERE id = ?1",
                rusqlite::params![id_owned, p, now],
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        }
        if let Some(active) = patch.is_active {
            conn.execute(
                "UPDATE provider_connections SET is_active = ?2, updated_at = ?3 WHERE id = ?1",
                rusqlite::params![id_owned, if active { 1 } else { 0 }, now],
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        }
        if patch.api_key.is_some() || patch.data.is_some() {
            let current: String = conn
                .query_row(
                    "SELECT data FROM provider_connections WHERE id = ?1",
                    [&id_owned],
                    |r| r.get(0),
                )
                .map_err(|e| Error::Db(e.to_string()))?;
            let mut data: serde_json::Value = serde_json::from_str(&current)?;
            if let Some(obj) = data.as_object_mut() {
                if let Some(k) = &patch.api_key {
                    obj.insert("apiKey".into(), serde_json::Value::String(k.clone()));
                }
                if let Some(serde_json::Value::Object(po)) = patch.data.clone() {
                    for (k, v) in po {
                        obj.insert(k, v);
                    }
                }
            }
            let data_str = serde_json::to_string(&data)?;
            conn.execute(
                "UPDATE provider_connections SET data = ?2, updated_at = ?3 WHERE id = ?1",
                rusqlite::params![id_owned, data_str, now],
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        }
        Ok(())
    })
    .await
}

pub async fn delete(db: &Db, id: &str) -> Result<()> {
    let id = id.to_string();
    db.call(move |conn| {
        conn.execute("DELETE FROM provider_connections WHERE id = ?1", [id])
            .map_err(|e| Error::Db(e.to_string()))?;
        Ok(())
    })
    .await
}

pub async fn get(db: &Db, id: &str) -> Result<Option<Connection>> {
    let id = id.to_string();
    db.call(move |conn| {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {COLUMNS} FROM provider_connections WHERE id = ?1"
            ))
            .map_err(|e| Error::Db(e.to_string()))?;
        let mut rows = stmt
            .query_map([&id], row_to_conn)
            .map_err(|e| Error::Db(e.to_string()))?;
        match rows.next() {
            Some(r) => Ok(Some(r.map_err(|e| Error::Db(e.to_string()))?)),
            None => Ok(None),
        }
    })
    .await
}

/// Mark a connection failed for a model: modelLock_<model> = ISO expiry,
/// backoffLevel bump, lastError. `deactivate` → is_active=false (credits exhausted).
pub async fn mark_unavailable(
    db: &Db,
    id: &str,
    model: &str,
    cooldown_ms: i64,
    error: &str,
    deactivate: bool,
) -> Result<()> {
    let id_owned = id.to_string();
    let model = model.to_string();
    let error = error.chars().take(300).collect::<String>();
    db.call(move |conn| {
        let now = Utc::now();
        let expiry = (now + chrono::Duration::milliseconds(cooldown_ms)).to_rfc3339();
        let current: String = conn
            .query_row("SELECT data FROM provider_connections WHERE id = ?1", [&id_owned], |r| r.get(0))
            .map_err(|e| Error::Db(e.to_string()))?;
        let mut data: serde_json::Value = serde_json::from_str(&current)?;
        let level = data.get("backoffLevel").and_then(|l| l.as_i64()).unwrap_or(0) + 1;
        if let Some(obj) = data.as_object_mut() {
            obj.insert(format!("modelLock_{model}"), serde_json::Value::String(expiry));
            obj.insert("backoffLevel".into(), serde_json::json!(level));
            obj.insert("lastError".into(), serde_json::Value::String(error));
            obj.insert("lastErrorAt".into(), serde_json::Value::String(now.to_rfc3339()));
        }
        let data_str = serde_json::to_string(&data)?;
        if deactivate {
            conn.execute(
                "UPDATE provider_connections SET data = ?2, is_active = 0, updated_at = ?3 WHERE id = ?1",
                rusqlite::params![id_owned, data_str, now.to_rfc3339()],
            )
        } else {
            conn.execute(
                "UPDATE provider_connections SET data = ?2, updated_at = ?3 WHERE id = ?1",
                rusqlite::params![id_owned, data_str, now.to_rfc3339()],
            )
        }
        .map_err(|e| Error::Db(e.to_string()))?;
        Ok(())
    })
    .await
}

/// Clear error state + succeeded model lock; bump usage stats.
pub async fn clear_error(db: &Db, id: &str, model: &str, sticky_limit: u32) -> Result<()> {
    let id_owned = id.to_string();
    let model = model.to_string();
    db.call(move |conn| {
        let now = Utc::now();
        let current: String = conn
            .query_row("SELECT data FROM provider_connections WHERE id = ?1", [&id_owned], |r| r.get(0))
            .map_err(|e| Error::Db(e.to_string()))?;
        let mut data: serde_json::Value = serde_json::from_str(&current)?;
        let last_used = data.get("lastUsedAt").and_then(|v| v.as_str()).map(String::from);
        let sticky = data.get("consecutiveUseCount").and_then(|c| c.as_i64()).unwrap_or(0);
        if let Some(obj) = data.as_object_mut() {
            obj.remove(&format!("modelLock_{model}"));
            obj.insert("backoffLevel".into(), serde_json::json!(0));
            obj.remove("lastError");
            obj.remove("lastErrorAt");
            // sticky round-robin counters
            let just_used = last_used
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                .map(|t| (now - t.with_timezone(&Utc)).num_minutes() < 30)
                .unwrap_or(false);
            let count = if just_used { sticky + 1 } else { 1 };
            obj.insert("consecutiveUseCount".into(), serde_json::json!(if (count as u32) < sticky_limit { count } else { 0 }));
            obj.insert("lastUsedAt".into(), serde_json::Value::String(now.to_rfc3339()));
        }
        let data_str = serde_json::to_string(&data)?;
        conn.execute(
            "UPDATE provider_connections SET data = ?2, updated_at = ?3 WHERE id = ?1",
            rusqlite::params![id_owned, data_str, now.to_rfc3339()],
        )
        .map_err(|e| Error::Db(e.to_string()))?;
        Ok(())
    })
    .await
}
