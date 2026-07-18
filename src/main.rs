//! Gausify local library server — a single binary that serves a splat library
//! (the `3dgs/`/`4dgs/<name>/manifest.json` layout) to the Gausify web app over
//! HTTPS, as a zero-config drop-in for the nginx setup.

mod config;
mod net;
mod serve;
mod tls;

use std::error::Error;
use std::future::pending;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum_server::tls_rustls::RustlsConfig;
use clap::Parser;

use crate::config::{Cli, Settings};
use crate::serve::{AppState, Stats};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gausify_server=info,tower_http=warn".into()),
        )
        .with_target(false)
        .init();

    // rustls has no built-in default provider once aws-lc-rs is disabled, so
    // install ring before any TLS config is built.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install the rustls ring crypto provider");

    let settings = Settings::resolve(Cli::parse())?;
    let ips = net::local_ipv4s();

    let state = AppState {
        root: Arc::new(settings.library.clone()),
        stats: Arc::new(Stats::default()),
    };
    let app = serve::router(state);

    // Copy the scalars each listener needs so the `async move` blocks don't
    // fight over ownership of `settings`.
    let (http_enabled, http_port) = (settings.http, settings.http_port);
    let (https_enabled, https_port) = (settings.https, settings.https_port);
    let library = settings.library.clone();
    let cert_ips = ips.clone();

    // A disabled listener becomes a future that never resolves, so the
    // `select!` still compiles and simply waits on the remaining ones.
    let http_future = {
        let app = app.clone();
        async move {
            if !http_enabled {
                return pending::<std::io::Result<()>>().await;
            }
            let addr = SocketAddr::from(([0, 0, 0, 0], http_port));
            axum_server::bind(addr).serve(app.into_make_service()).await
        }
    };

    let https_future = {
        let app = app.clone();
        async move {
            if !https_enabled {
                return pending::<std::io::Result<()>>().await;
            }
            let paths = tls::ensure_cert(&library, &cert_ips)
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            let tls_config = RustlsConfig::from_pem_file(&paths.cert, &paths.key).await?;
            let addr = SocketAddr::from(([0, 0, 0, 0], https_port));
            axum_server::bind_rustls(addr, tls_config)
                .serve(app.into_make_service())
                .await
        }
    };

    print_banner(&settings, &ips);

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("shutting down");
            Ok(())
        }
        result = http_future => result.map_err(|e| format!("HTTP listener failed: {e}").into()),
        result = https_future => result.map_err(|e| format!("HTTPS listener failed: {e}").into()),
    }
}

fn print_banner(settings: &Settings, ips: &[Ipv4Addr]) {
    println!();
    println!("  Gausify Server");
    println!("  Serving library:");
    println!("    {}", settings.library.display());
    println!();
    if settings.https {
        println!("  HTTPS:");
        println!("    https://localhost:{}", settings.https_port);
        for ip in ips {
            println!("    https://{ip}:{}", settings.https_port);
        }
    }
    if settings.http {
        println!("  HTTP:");
        println!("    http://localhost:{}", settings.http_port);
        for ip in ips {
            println!("    http://{ip}:{}", settings.http_port);
        }
    }
    println!();
    println!("  Paste an HTTPS URL above into the Gausify gallery. Ctrl-C to stop.");
    println!();
}
