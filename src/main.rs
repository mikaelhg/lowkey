use std::net::SocketAddr;

use anyhow::Context;
use lowkey::config::Config;
use lowkey::service::{hash_password, is_password_hash};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

const USAGE: &str = "usage: lowkey [hash-password]";

fn main() -> anyhow::Result<()> {
    match std::env::args().nth(1).as_deref() {
        None => serve(),
        Some("hash-password") => print_password_hash(),
        Some(_) => anyhow::bail!(USAGE),
    }
}

/// Reads a password from stdin and prints its hash, for the
/// `admin_password_hash` setting.
fn print_password_hash() -> anyhow::Result<()> {
    eprintln!("Enter the password, then press Enter:");
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .context("reading password")?;
    let password = line.trim_end_matches(['\r', '\n']);
    anyhow::ensure!(!password.is_empty(), "password must not be empty");
    println!("{}", hash_password(password));
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn serve() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Config::load().context("loading configuration")?;
    match &config.admin_password_hash {
        Some(hash) => anyhow::ensure!(
            is_password_hash(hash),
            "admin_password_hash is not a valid password hash; create one with `lowkey hash-password`"
        ),
        None => tracing::warn!("no admin_password_hash configured, admin account disabled"),
    }

    let app = lowkey::build_app(&config)
        .await
        .with_context(|| format!("opening data directory {}", config.data_dir.display()))?;

    let listener = TcpListener::bind(config.listen)
        .await
        .with_context(|| format!("binding to {}", config.listen))?;
    tracing::info!(listen = %config.listen, data_dir = %config.data_dir.display(), "lowkey started");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("serving HTTP")?;

    tracing::info!("lowkey stopped");
    Ok(())
}

/// Resolves on Ctrl-C or, on Unix, SIGTERM (as sent by systemd).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
