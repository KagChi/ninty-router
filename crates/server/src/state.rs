use crate::db::Db;

pub struct AppState {
    pub db: Db,
    pub http: reqwest::Client,
    request_logs: std::sync::atomic::AtomicBool,
    pub pxpipe_installing: std::sync::atomic::AtomicBool,
}

impl AppState {
    pub fn enable_request_logs(&self) -> bool {
        self.request_logs.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn set_request_logs(&self, on: bool) {
        self.request_logs
            .store(on, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn new(db: Db) -> Self {
        let http = engine::executor::build_client().unwrap_or_else(|_| reqwest::Client::new());
        Self {
            db,
            http,
            pxpipe_installing: std::sync::atomic::AtomicBool::new(false),
            request_logs: std::sync::atomic::AtomicBool::new(false),
        }
    }
}
