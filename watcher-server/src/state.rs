use std::sync::Arc;

use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct AppState(Arc<Inner>);
impl AppState {
    pub fn new() -> Self {
        Self(Arc::new(Inner::new()))
    }
}
impl std::ops::Deref for AppState {
    type Target = Inner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct Inner {
    pub assets: Arc<RwLock<Vec<(u64, String)>>>, // (created, filename)
}
impl Inner {
    pub fn new() -> Self {
        Self {
            assets: Arc::new(RwLock::new(vec![])),
        }
    }
}
