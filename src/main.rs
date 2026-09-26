use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod auth;
mod config;
mod db;
mod driver;
mod interactive;
mod model;
pub mod preview;
mod server;
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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
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

    // Default to 'interactive' console if no subcommand provided
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
            let pool = db::init_db(&config.resolved_db_path(&data_dir)).await?;
            interactive::run_interactive_console(&pool, &data_dir).await?;
        }
        Commands::Version => {
            println!("Version: v{}", env!("CARGO_PKG_VERSION"));
            println!(
                "OS/Arch: {}/{}",
                std::env::consts::OS,
                std::env::consts::ARCH
            );
        }
        Commands::Server(server_args) => {
            let (mut config, config_path) = config::Config::load_or_create(&data_dir)?;
            if let Some(port) = server_args.port {
                config.scheme.http_port = port;
            }
            if let Some(host) = server_args.host {
                config.scheme.address = host;
            }

            info!("loaded configuration from {:?}", config_path);

            let db_path = config.resolved_db_path(&data_dir);
            let is_new_database = !db_path.exists();
            let home_path = if is_new_database {
                let home_path = std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .context("HOME is not configured for the server process")?
                    .canonicalize()
                    .context("failed to resolve the server process HOME directory")?;
                if !home_path.is_dir() {
                    bail!("the server process HOME directory is not a directory: {home_path:?}");
                }
                Some(home_path)
            } else {
                None
            };
            info!("initializing database at {:?}", db_path);
            let pool = db::init_db(&db_path).await?;

            if let Some(home_path) = home_path {
                let addition = serde_json::to_string(&driver::local::LocalAddition {
                    root_folder_path: home_path.to_string_lossy().into_owned(),
                    show_hidden: false,
                })?;
                sqlx::query(
                    "INSERT OR IGNORE INTO `x_storages` (`mount_path`, `driver`, `addition`) VALUES ('/', 'Local', ?)",
                )
                .bind(addition)
                .execute(&pool)
                .await?;
                info!("mounted server process HOME directory at /");
            }

            info!("loading storage manager...");
            let storage = driver::StorageManager::load_from_db(&pool).await?;

            server::run_server(config, pool, storage).await?;
        }
    }

    Ok(())
}
