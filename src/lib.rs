#[cfg(not(target_os = "linux"))]
compile_error!("Rulist only supports Linux.");

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use std::{io, path::PathBuf};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub mod auth;
pub mod config;
pub mod db;
pub mod driver;
pub mod interactive;
pub mod model;
pub mod preview;
pub mod server;
mod sign;
mod static_files;

#[derive(Parser, Debug)]
#[command(
    name = "rulist",
    author,
    version = env!("CARGO_PKG_VERSION"),
    about = "A lightweight, high-performance file listing tool written in Rust (Rulist)"
)]
struct Cli {
    /// Data directory path
    #[arg(
        short,
        long,
        global = true,
        env = "RULIST_DATA_DIR",
        default_value = "data"
    )]
    data_dir: PathBuf,

    /// Enable debug log level
    #[arg(long, global = true)]
    debug: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Interactive management console (default)
    #[command(alias = "menu", alias = "manage", alias = "console", alias = "i")]
    Interactive,

    /// Run Rulist HTTP server
    #[command(alias = "serve")]
    Server(ServerArgs),

    /// Display version and build information
    Version,
}

#[derive(Args, Debug)]
struct ServerArgs {
    /// HTTP listen port
    #[arg(short, long)]
    port: Option<u16>,

    /// HTTP listen host address
    #[arg(long)]
    host: Option<String>,
}

fn initial_home_path() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME is not set; cannot initialize the default storage mount")?;
    let home = home
        .canonicalize()
        .with_context(|| format!("failed to resolve current user's HOME directory: {home:?}"))?;
    if !home.is_dir() {
        anyhow::bail!("current user's HOME is not a directory: {home:?}");
    }
    Ok(home)
}

async fn init_database(config: &config::Config, data_dir: &std::path::Path) -> Result<db::DbPool> {
    let db_path = config.resolved_db_path(data_dir);
    let home_path = initial_home_path()?;

    info!("initializing database at {:?}", db_path);
    let pool = db::init_db(&db_path, &home_path).await?;

    Ok(pool)
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();

    let filter = if cli.debug {
        "rulist=debug,tower_http=debug,axum=debug"
    } else {
        "rulist=info,tower_http=info"
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let command = cli.command.unwrap_or(Commands::Interactive);

    let data_dir = if cli.data_dir == std::path::Path::new("data") && !cli.data_dir.exists() {
        if let Ok(home) = std::env::var("HOME") {
            let user_rulist = PathBuf::from(home).join(".rulist").join("data");
            if user_rulist.exists() {
                user_rulist
            } else {
                cli.data_dir
            }
        } else {
            cli.data_dir
        }
    } else {
        cli.data_dir
    };

    match command {
        Commands::Interactive => {
            let (config, _) = config::Config::load_or_create(&data_dir)?;
            let pool = init_database(&config, &data_dir).await?;
            if let Err(error) = interactive::run_interactive_console(&pool, &data_dir).await {
                if error
                    .downcast_ref::<io::Error>()
                    .is_some_and(|error| error.kind() == io::ErrorKind::UnexpectedEof)
                {
                    println!("\n已退出 Rulist 交互控制台。");
                } else {
                    return Err(error);
                }
            }
        }
        Commands::Version => {
            println!("Version: v{}", env!("CARGO_PKG_VERSION"));
        }
        Commands::Server(server_args) => {
            let (mut config, config_path) = config::Config::load_or_create(&data_dir)?;
            if let Some(port) = server_args.port {
                config.server.port = port;
            }
            if let Some(host) = server_args.host {
                config.server.address = host;
            }

            info!("loaded configuration from {:?}", config_path);

            let pool = init_database(&config, &data_dir).await?;

            info!("loading storage manager...");
            let storage = driver::StorageManager::load_from_db(&pool).await?;

            server::run_server(config, pool, storage).await?;
        }
    }

    Ok(())
}
