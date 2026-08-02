use crate::db::Db;

pub struct AppState {
    pub db: Db,
    pub http: reqwest::Client,
    request_logs: std::sync::atomic::AtomicBool,
}

impl AppState {
    pub fn enable_request_logs(&self) -> bool {
        self.request_logs.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn new(db: Db) -> Self {
        let http = engine::executor::build_client().unwrap_or_else(|_| reqwest::Client::new());
        Self { db, http, request_logs: std::sync::atomic::AtomicBool::new(false) }
    }
}
