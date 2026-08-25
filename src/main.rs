//! FakApp — find out the service is dead before the fans do.
//!
//! One binary, three jobs: probe a list of URLs on an interval, decide UP or
//! DOWN with hysteresis (a single blip is not an outage), and mail a human on
//! every transition — plus periodic reminders for as long as something stays
//! down. A read-only dashboard serves the same state over HTTP.

mod checker;
mod config;
mod discord;
mod mailer;
mod state;
mod store;
mod web;

use std::sync::Arc;

use tokio::sync::Mutex;

pub use config::Config;

/// Shared monitor state: the checker tasks write it, the web handlers read it.
pub type Shared = Arc<Mutex<state::MonitorState>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Deploy scripts verify the shipped binary by version before switching.
    if std::env::args().any(|arg| arg == "--version") {
        println!("fakap {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .compact()
        .init();

    let config_path = std::env::var("FAKAP_CONFIG").unwrap_or_else(|_| "fakap.json".to_owned());
    let config = Config::load(&config_path)
        .map_err(|error| anyhow::anyhow!("invalid config {config_path}: {error:#}"))?;
    tracing::info!(targets = config.targets.len(), "fakap starting");

    let previous = store::load(&config.state_file)?;
    let shared: Shared = Arc::new(Mutex::new(state::MonitorState::restore(&config, previous)));

    // One HTTP client for every probe and Discord alert. A failed build means
    // nothing can ever be probed or paged: refuse to start rather than run a
    // decorative watchdog.
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent(concat!("fakap/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| anyhow::anyhow!("probe/alert http client unavailable: {error}"))?;
    // One SMTP transport, built once; alerts are rare by design and must not
    // pay connection setup more than necessary.
    let mailer = match config.mailer() {
        Some(smtp) => Some(Arc::new(mailer::Mailer::new(smtp).map_err(|error| {
            anyhow::anyhow!("mail transport unavailable: {error:#}")
        })?)),
        None => None,
    };

    for target in &config.targets {
        let task = checker::Checker {
            config: config.clone(),
            state: Arc::clone(&shared),
            target_id: target.id.clone(),
            client: http.clone(),
            mailer: mailer.clone(),
        };
        tokio::spawn(task.run());
    }
    tokio::spawn(store::saver(Arc::clone(&shared), config.state_file.clone()));

    let app = web::router(shared, config.clone());
    let listen = config.listen.clone();
    let listener = tokio::net::TcpListener::bind(&listen).await?;
    tracing::info!(address = %listen, "dashboard listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut signal) = signal(SignalKind::terminate()) {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}
