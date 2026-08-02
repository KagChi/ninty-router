use chrono::Utc;
use ninty_core::error::{Error, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::Db;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Combo {
    pub id: String,
    pub name: String,
    pub kind: Option<String>,
    /// Ordered list of "provider/model" specs.
    pub models: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewCombo {
    pub name: String,
    pub kind: Option<String>,
    pub models: Vec<String>,
}

const COLUMNS: &str = "id, name, kind, models, created_at, updated_at";

fn row_to_combo(row: &rusqlite::Row<'_>) -> rusqlite::Result<Combo> {
    let models: String = row.get("models")?;
    Ok(Combo {
        id: row.get("id")?,
        name: row.get("name")?,
        kind: row.get("kind")?,
        models: serde_json::from_str(&models).unwrap_or_default(),
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

pub async fn list(db: &Db) -> Result<Vec<Combo>> {
    db.call(|conn| {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {COLUMNS} FROM combos ORDER BY created_at ASC"
            ))
            .map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt
            .query_map([], row_to_combo)
            .map_err(|e| Error::Db(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Db(e.to_string()))?;
        Ok(rows)
    })
    .await
}

pub async fn get_by_name(db: &Db, name: &str) -> Result<Option<Combo>> {
    let name = name.to_string();
    db.call(move |conn| {
        let mut stmt = conn
            .prepare(&format!("SELECT {COLUMNS} FROM combos WHERE name = ?1"))
            .map_err(|e| Error::Db(e.to_string()))?;
        let mut rows = stmt
            .query_map([&name], row_to_combo)
            .map_err(|e| Error::Db(e.to_string()))?;
        match rows.next() {
            Some(r) => Ok(Some(r.map_err(|e| Error::Db(e.to_string()))?)),
            None => Ok(None),
        }
    })
    .await
}

pub async fn create(db: &Db, new: NewCombo) -> Result<Combo> {
    if new.name.trim().is_empty() || new.models.is_empty() {
        return Err(Error::BadRequest("name and models required".into()));
    }
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let models = serde_json::to_string(&new.models)?;
    let name = new.name.trim().to_string();
    let kind = new.kind.clone();
    db.call(move |conn| {
        conn.execute(
            "INSERT INTO combos (id, name, kind, models, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            rusqlite::params![id, name, kind, models, now],
        )
        .map_err(|e| Error::Db(e.to_string()))?;
        Ok(Combo {
            id,
            name,
            kind,
            models: new.models,
            created_at: now.clone(),
            updated_at: now,
        })
    })
    .await
}

pub async fn update(db: &Db, id: &str, patch: NewCombo) -> Result<()> {
    let id = id.to_string();
    let models = serde_json::to_string(&patch.models)?;
    db.call(move |conn| {
        conn.execute(
            "UPDATE combos SET name = ?2, kind = ?3, models = ?4, updated_at = ?5 WHERE id = ?1",
            rusqlite::params![id, patch.name, patch.kind, models, Utc::now().to_rfc3339()],
        )
        .map_err(|e| Error::Db(e.to_string()))?;
        Ok(())
    })
    .await
}

pub async fn delete(db: &Db, id: &str) -> Result<()> {
    let id = id.to_string();
    db.call(move |conn| {
        conn.execute("DELETE FROM combos WHERE id = ?1", [id])
            .map_err(|e| Error::Db(e.to_string()))?;
        Ok(())
    })
    .await
}
