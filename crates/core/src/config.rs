use std::path::PathBuf;

pub const DEFAULT_PORT: u16 = 20128;

/// Resolve the data directory: $DATA_DIR or ~/.ninty-router
pub fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("DATA_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".ninty-router")
}

pub fn db_path() -> PathBuf {
    data_dir().join("db").join("data.sqlite")
}

/// Secret used for API-key CRC + session signing.
pub fn api_key_secret() -> String {
    std::env::var("API_KEY_SECRET").unwrap_or_else(|_| "ninty-router-api-key-secret".to_string())
}

/// Consistent machine id: /etc/machine-id, else hostname, hashed to 16 hex chars.
pub fn machine_id() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let raw = std::fs::read_to_string("/etc/machine-id")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::process::Command::new("hostname")
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "ninty-unknown-host".to_string());

    let mut h = DefaultHasher::new();
    raw.hash(&mut h);
    format!("{:016x}", h.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_id_is_16_hex() {
        let id = machine_id();
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
