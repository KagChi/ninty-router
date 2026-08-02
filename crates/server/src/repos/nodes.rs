use chrono::Utc;
use ninty_core::error::{Error, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::Db;

/// Custom OpenAI-compatible endpoint. data: {prefix, baseUrl, apiKey, models?[]}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub name: Option<String>,
    pub data: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

impl Node {
    pub fn prefix(&self) -> Option<&str> {
        self.data.get("prefix").and_then(|v| v.as_str())
    }
    pub fn base_url(&self) -> Option<&str> {
        self.data.get("baseUrl").and_then(|v| v.as_str())
    }
    pub fn api_key(&self) -> Option<&str> {
        self.data.get("apiKey").and_then(|v| v.as_str())
    }

    /// "openai" (default) or "anthropic".
    pub fn api_type(&self) -> &str {
        self.data
            .get("apiType")
            .and_then(|v| v.as_str())
            .unwrap_or("openai")
    }

    /// Endpoint URL for this node (chat/completions or messages by apiType).
    pub fn chat_url(&self) -> Option<String> {
        let base = self.base_url()?.trim_end_matches('/');
        let (completions, messages) = ("/chat/completions", "/messages");
        if self.api_type() == "anthropic" {
            if base.ends_with(messages) {
                Some(base.to_string())
            } else if base.ends_with("/v1") {
                Some(format!("{base}{messages}"))
            } else {
                Some(format!("{base}/v1{messages}"))
            }
        } else if base.ends_with(completions) {
            Some(base.to_string())
        } else {
            Some(format!("{base}{completions}"))
        }
    }

    pub fn sanitized(&self) -> Self {
        let mut n = self.clone();
        if let Some(obj) = n.data.as_object_mut() {
            if let Some(serde_json::Value::String(s)) = obj.get_mut("apiKey") {
                let tail: String = s
                    .chars()
                    .rev()
                    .take(4)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect();
                *s = format!("****{tail}");
            }
        }
        n
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewNode {
    pub name: Option<String>,
    pub prefix: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub api_type: Option<String>,
}

const COLUMNS: &str = "id, type, name, data, created_at, updated_at";

fn row_to_node(row: &rusqlite::Row<'_>) -> rusqlite::Result<Node> {
    let data: String = row.get("data")?;
    Ok(Node {
        id: row.get("id")?,
        node_type: row.get("type")?,
        name: row.get("name")?,
        data: serde_json::from_str(&data).unwrap_or(serde_json::json!({})),
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

pub async fn list(db: &Db) -> Result<Vec<Node>> {
    db.call(move |conn| {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {COLUMNS} FROM provider_nodes ORDER BY created_at ASC"
            ))
            .map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt
            .query_map([], row_to_node)
            .map_err(|e| Error::Db(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Db(e.to_string()))?;
        Ok(rows)
    })
    .await
}

pub async fn create(db: &Db, new: NewNode) -> Result<Node> {
    if new.prefix.is_empty() || new.prefix.contains('/') {
        return Err(Error::BadRequest(
            "prefix must be non-empty without '/'".into(),
        ));
    }
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let data = serde_json::json!({
        "prefix": new.prefix,
        "baseUrl": new.base_url,
        "apiKey": new.api_key,
        "apiType": new.api_type.as_deref().unwrap_or("openai"),
    });
    let data_str = serde_json::to_string(&data)?;
    let name = new.name.clone();
    db.call(move |conn| {
        conn.execute(
            "INSERT INTO provider_nodes (id, type, name, data, created_at, updated_at)
             VALUES (?1, 'openai', ?2, ?3, ?4, ?4)",
            rusqlite::params![id, name, data_str, now],
        )
        .map_err(|e| Error::Db(e.to_string()))?;
        Ok(Node {
            id,
            node_type: "openai".into(),
            name,
            data,
            created_at: now.clone(),
            updated_at: now,
        })
    })
    .await
}

pub async fn delete(db: &Db, id: &str) -> Result<()> {
    let id = id.to_string();
    db.call(move |conn| {
        conn.execute("DELETE FROM provider_nodes WHERE id = ?1", [id])
            .map_err(|e| Error::Db(e.to_string()))?;
        Ok(())
    })
    .await
}
