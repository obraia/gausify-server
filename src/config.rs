//! CLI flags + optional `gausify.toml`, merged into the effective [`Settings`].
//!
//! Precedence: command-line flag > `gausify.toml` value > built-in default.

use std::error::Error;
use std::path::PathBuf;

use clap::Parser;
use serde::Deserialize;

/// Drop-in local server for a Gausify splat library.
#[derive(Parser, Debug)]
#[command(name = "gausify-server", version, about)]
pub struct Cli {
    /// Library root to serve (default: the current working directory).
    #[arg(short, long)]
    pub library: Option<PathBuf>,

    /// Port for plain HTTP (default: 8080).
    #[arg(long)]
    pub http_port: Option<u16>,

    /// Port for HTTPS (default: 8443).
    #[arg(long)]
    pub https_port: Option<u16>,

    /// Disable the HTTPS listener.
    #[arg(long, default_value_t = false)]
    pub no_https: bool,

    /// Disable the plain-HTTP listener.
    #[arg(long, default_value_t = false)]
    pub no_http: bool,

    /// Path to a config file (default: `<cwd>/gausify.toml`, if present).
    #[arg(long)]
    pub config: Option<PathBuf>,
}

/// `gausify.toml` — every field optional so a bare or missing file is valid.
#[derive(Deserialize, Default)]
#[serde(default)]
struct FileConfig {
    library: Option<String>,
    http: Option<bool>,
    https: Option<bool>,
    http_port: Option<u16>,
    https_port: Option<u16>,
}

/// Fully resolved runtime settings.
pub struct Settings {
    /// Canonical, existing directory the server serves.
    pub library: PathBuf,
    pub http: bool,
    pub https: bool,
    pub http_port: u16,
    pub https_port: u16,
}

impl Settings {
    pub fn resolve(cli: Cli) -> Result<Settings, Box<dyn Error>> {
        let config_path = cli
            .config
            .clone()
            .unwrap_or_else(|| PathBuf::from("gausify.toml"));
        let file: FileConfig = match std::fs::read_to_string(&config_path) {
            Ok(text) => toml::from_str(&text)?,
            Err(_) => FileConfig::default(),
        };

        let library = cli
            .library
            .or_else(|| file.library.map(PathBuf::from))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let library = std::fs::canonicalize(&library)
            .map_err(|e| format!("library path {library:?} is not accessible: {e}"))?;
        if !library.is_dir() {
            return Err(format!("library path {library:?} is not a directory").into());
        }

        Ok(Settings {
            library,
            http: !cli.no_http && file.http.unwrap_or(true),
            https: !cli.no_https && file.https.unwrap_or(true),
            http_port: cli.http_port.or(file.http_port).unwrap_or(8080),
            https_port: cli.https_port.or(file.https_port).unwrap_or(8443),
        })
    }
}
