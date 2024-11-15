use std::time::{Duration, UNIX_EPOCH};

use crate::{config::config, state::AppState};

type Error = Box<dyn std::error::Error + Send + Sync>;

pub async fn sync_assets(state: AppState, period: Duration) -> Result<(), Error> {
    let mut interval = tokio::time::interval(period);

    loop {
        interval.tick().await;
        let mut paths = tokio::fs::read_dir(&config().assets_dir).await.unwrap();
        let mut assets = state.assets.write().await;
        while let Some(entry) = paths.next_entry().await.unwrap() {
            let created = entry
                .metadata()
                .await
                .unwrap()
                .created()
                .unwrap()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            if assets.iter().any(|(c, _)| c == &created) {
                continue;
            }
            assets.push((created, entry.file_name().into_string().unwrap()));
        }
        assets.sort_by(|a, b| a.0.cmp(&b.0));

        tracing::info!("synced assets from dir {:?}", config().assets_dir);
    }
}
