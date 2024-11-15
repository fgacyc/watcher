use std::{path::PathBuf, sync::OnceLock};

use clap::Parser;

/// Parse the configuration. Note that this will only occur once due to the
/// `OnceLock` being used here.
pub fn config() -> &'static Config {
    static CONFIG: OnceLock<Config> = OnceLock::new();
    CONFIG.get_or_init(|| {
        let config = Config::parse();
        tracing::info!("Configuration parsed successfully: {:?}", config);
        config
    })
}

/// The configuration parameters for the application.
///
/// These can either be passed on the command line, or pulled from environment variables.
/// The latter is preferred as environment variables are one of the recommended ways to
/// get configuration from Kubernetes Secrets in deployment.
///
/// For development convenience, these can also be read from a `.env` file in the working
/// directory where the application is started.
///
/// See `.env.example` in the repository root for details.
#[derive(clap::Parser, Clone)]
pub struct Config {
    /// Address or host that the server should listen on.
    #[clap(long, env, default_value = "0.0.0.0")]
    pub address: String,

    /// Port that the application should listen on.
    #[clap(long, env, default_value_t = 8000)]
    pub port: u16,

    /// Port that the application should listen on.
    #[clap(long, env)]
    pub assets_dir: PathBuf,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("address", &self.address)
            .field("port", &self.port)
            .field("assets_dir", &self.assets_dir)
            .finish()
    }
}
