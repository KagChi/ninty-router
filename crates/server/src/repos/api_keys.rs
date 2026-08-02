use chrono::Utc;
use hmac::{Hmac, Mac};
use ninty_core::error::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

use crate::db::Db;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: String,
    pub key: String,
    pub name: Option<String>,
    pub is_active: bool,
    pub token_limit: Option<i64>,
    pub limit_window: Option<String>,
    pub rpm_limit: Option<i64>,
    pub allowed_models: Vec<String>,
    pub limit_reset_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewApiKey {
    pub name: Option<String>,
    pub token_limit: Option<i64>,
    pub limit_window: Option<String>,
    pub rpm_limit: Option<i64>,
    pub allowed_models: Option<Vec<String>>,
}

fn crc8(input: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(ninty_core::config::api_key_secret().as_bytes())
        .expect("hmac accepts any key length");
    mac.update(input.as_bytes());
    let out = mac.finalize().into_bytes();
    hex::encode(&out[..4])
}

/// `sk-{machine16}-{id6}-{crc8}` — crc over machine+id, like the reference.
pub fn generate_key(machine_id: &str, key_id: &str) -> String {
    let id6 = &key_id.chars().take(6).collect::<String>();
    let crc = crc8(&format!("{machine_id}{id6}"));
    format!("sk-{machine_id}-{id6}-{crc}")
}

fn row_to_key(row: &rusqlite::Row<'_>) -> rusqlite::Result<ApiKey> {
    let allowed: String = row.get("allowed_models")?;
    Ok(ApiKey {
        id: row.get("id")?,
        key: row.get("key")?,
        name: row.get("name")?,
        is_active: row.get::<_, i64>("is_active")? == 1,
        token_limit: row.get("token_limit")?,
        limit_window: row.get("limit_window")?,
        rpm_limit: row.get("rpm_limit")?,
        allowed_models: serde_json::from_str(&allowed).unwrap_or_default(),
        limit_reset_at: row.get("limit_reset_at")?,
        created_at: row.get("created_at")?,
    })
}

const COLUMNS: &str = "id, key, name, is_active, token_limit, limit_window, rpm_limit, allowed_models, limit_reset_at, created_at";

pub async fn list(db: &Db) -> Result<Vec<ApiKey>> {
    db.call(|conn| {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {COLUMNS} FROM api_keys ORDER BY created_at DESC"
            ))
            .map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt
            .query_map([], row_to_key)
            .map_err(|e| Error::Db(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Db(e.to_string()))?;
        Ok(rows)
    })
    .await
}

pub async fn create(db: &Db, new: NewApiKey) -> Result<ApiKey> {
    let id = Uuid::new_v4().to_string();
    let machine = ninty_core::config::machine_id();
    let key = generate_key(&machine, &id);
    let now = Utc::now().to_rfc3339();
    let allowed = serde_json::to_string(&new.allowed_models.unwrap_or_default())?;
    let name = new.name.clone();
    db.call(move |conn| {
        conn.execute(
            "INSERT INTO api_keys (id, key, name, machine_id, is_active, token_limit, limit_window, rpm_limit, allowed_models, limit_reset_at, created_at)
             VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7, ?8, NULL, ?9)",
            rusqlite::params![
                id, key, name, machine, new.token_limit, new.limit_window, new.rpm_limit, allowed, now
            ],
        )
        .map_err(|e| Error::Db(e.to_string()))?;
        Ok(ApiKey {
            id,
            key,
            name,
            is_active: true,
            token_limit: new.token_limit,
            limit_window: new.limit_window,
            rpm_limit: new.rpm_limit,
            allowed_models: serde_json::from_str(&allowed)?,
            limit_reset_at: None,
            created_at: now,
        })
    })
    .await
}

pub async fn get_by_key(db: &Db, key: &str) -> Result<Option<ApiKey>> {
    let key = key.to_string();
    db.call(move |conn| {
        let mut stmt = conn
            .prepare(&format!("SELECT {COLUMNS} FROM api_keys WHERE key = ?1"))
            .map_err(|e| Error::Db(e.to_string()))?;
        let mut rows = stmt
            .query_map([&key], row_to_key)
            .map_err(|e| Error::Db(e.to_string()))?;
        match rows.next() {
            Some(r) => Ok(Some(r.map_err(|e| Error::Db(e.to_string()))?)),
            None => Ok(None),
        }
    })
    .await
}

pub async fn update(db: &Db, id: &str, patch: NewApiKey, is_active: Option<bool>) -> Result<()> {
    let id = id.to_string();
    db.call(move |conn| {
        if let Some(name) = &patch.name {
            conn.execute(
                "UPDATE api_keys SET name = ?2 WHERE id = ?1",
                rusqlite::params![id, name],
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        }
        if patch.token_limit.is_some() {
            conn.execute(
                "UPDATE api_keys SET token_limit = ?2 WHERE id = ?1",
                rusqlite::params![id, patch.token_limit],
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        }
        if let Some(w) = &patch.limit_window {
            conn.execute(
                "UPDATE api_keys SET limit_window = ?2 WHERE id = ?1",
                rusqlite::params![id, w],
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        }
        if patch.rpm_limit.is_some() {
            conn.execute(
                "UPDATE api_keys SET rpm_limit = ?2 WHERE id = ?1",
                rusqlite::params![id, patch.rpm_limit],
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        }
        if let Some(models) = &patch.allowed_models {
            let json = serde_json::to_string(models)?;
            conn.execute(
                "UPDATE api_keys SET allowed_models = ?2 WHERE id = ?1",
                rusqlite::params![id, json],
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        }
        if let Some(active) = is_active {
            conn.execute(
                "UPDATE api_keys SET is_active = ?2 WHERE id = ?1",
                rusqlite::params![id, if active { 1 } else { 0 }],
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
        conn.execute("DELETE FROM api_keys WHERE id = ?1", [id])
            .map_err(|e| Error::Db(e.to_string()))?;
        Ok(())
    })
    .await
}

pub async fn reset_limit(db: &Db, id: &str) -> Result<()> {
    let id = id.to_string();
    db.call(move |conn| {
        conn.execute(
            "UPDATE api_keys SET limit_reset_at = NULL WHERE id = ?1",
            [id],
        )
        .map_err(|e| Error::Db(e.to_string()))?;
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_format_and_crc_stable() {
        let k1 = generate_key("abcdef0123456789", "uuid-123456");
        assert!(k1.starts_with("sk-abcdef0123456789-uuid-1"));
        assert_eq!(k1.len(), "sk-".len() + 16 + 1 + 6 + 1 + 8);
        let k2 = generate_key("abcdef0123456789", "uuid-123456");
        assert_eq!(k1, k2);
    }

    #[tokio::test]
    async fn crud_roundtrip() {
        let db = Db::open_memory().unwrap();
        let k = create(
            &db,
            NewApiKey {
                name: Some("test".into()),
                rpm_limit: Some(60),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(k.is_active);
        let fetched = get_by_key(&db, &k.key).await.unwrap().unwrap();
        assert_eq!(fetched.id, k.id);
        update(&db, &k.id, NewApiKey::default(), Some(false))
            .await
            .unwrap();
        let all = list(&db).await.unwrap();
        assert_eq!(all.len(), 1);
        assert!(!all[0].is_active);
        delete(&db, &k.id).await.unwrap();
        assert!(list(&db).await.unwrap().is_empty());
    }
}
