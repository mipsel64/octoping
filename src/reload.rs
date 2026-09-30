use std::{
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use tracing::{info, warn};

use crate::{app::App, config::Config, error::Result, server::Shared};

/// Re-reads the config file every `reload_secs` and swaps in a new app when its content changes.
// Polling, not a file watcher: k8s swaps ConfigMap volumes via a `..data` symlink, which watchers easily miss.
pub async fn watch(path: PathBuf, mut last: String, shared: Arc<Shared>) {
    loop {
        let every = shared
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .reload_every;
        tokio::time::sleep(every).await;
        reload(&path, &mut last, &shared);
    }
}

fn reload(path: &Path, last: &mut String, shared: &Shared) {
    let text = match Config::read(path) {
        Ok(text) if text != *last => text,
        Ok(_) => return,
        Err(e) => {
            warn!("{e}");
            return;
        }
    };
    match build(&text) {
        Ok(app) => {
            let mut current = shared.write().unwrap_or_else(|e| e.into_inner());
            if app.listen != current.listen {
                warn!(listen = %app.listen, "listen change needs a restart");
            }
            *current = Arc::new(app);
            info!("config reloaded");
        }
        Err(e) => warn!("keeping previous config: {e}"),
    }
    *last = text;
}

fn build(text: &str) -> Result<App> {
    App::new(Config::parse(text)?)
}

pub fn shared(app: App) -> Arc<Shared> {
    Arc::new(RwLock::new(Arc::new(app)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = include_str!("../octoping.example.yaml");

    #[test]
    fn swaps_app_on_change_and_keeps_it_on_error() {
        let path =
            std::env::temp_dir().join(format!("octoping-reload-{}.yaml", std::process::id()));
        let with_secret = |s: &str| EXAMPLE.replace("${GITHUB_WEBHOOK_SECRET}", s);
        let mut last = with_secret("one");
        let shared = shared(build(&last).unwrap());
        let secret = || shared.read().unwrap().secret.clone();

        std::fs::write(&path, with_secret("two")).unwrap();
        reload(&path, &mut last, &shared);
        assert_eq!(secret(), "two");

        std::fs::write(&path, "events: [").unwrap();
        reload(&path, &mut last, &shared);
        assert_eq!(secret(), "two");
        assert_eq!(last, "events: [");

        std::fs::remove_file(&path).unwrap();
        reload(&path, &mut last, &shared);
        assert_eq!(secret(), "two");
    }
}
