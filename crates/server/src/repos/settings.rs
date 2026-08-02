use ninty_core::error::Result;
use ninty_core::settings::Settings;

use crate::db::Db;

pub async fn get(db: &Db) -> Result<Settings> {
    db.call(|conn| {
        let data: Option<String> = conn
            .query_row("SELECT data FROM settings WHERE id = 1", [], |r| r.get(0))
            .ok();
        match data {
            Some(json) => {
                let s: Settings = serde_json::from_str(&json)?;
                Ok(s)
            }
            None => Ok(Settings::default()),
        }
    })
    .await
}

pub async fn put(db: &Db, settings: &Settings) -> Result<()> {
    let json = serde_json::to_string(settings)?;
    db.call(move |conn| {
        conn.execute(
            "INSERT INTO settings (id, data) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET data = excluded.data",
            [json],
        )
        .map_err(|e| ninty_core::error::Error::Db(e.to_string()))?;
        Ok(())
    })
    .await
}

/// Patch settings with a partial JSON object. `password_hash` is never set via patch
/// (use the dedicated password endpoint).
pub async fn patch(db: &Db, patch: serde_json::Value) -> Result<Settings> {
    let mut current = get(db).await?;
    let mut full = serde_json::to_value(&current)?;
    if let (Some(map), Some(patch_map)) = (full.as_object_mut(), patch.as_object()) {
        for (k, v) in patch_map {
            if k == "password_hash" {
                continue;
            }
            map.insert(k.clone(), v.clone());
        }
    }
    current = serde_json::from_value(full)?;
    put(db, &current).await?;
    Ok(current)
}
