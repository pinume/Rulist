use std::path::PathBuf;
use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod auth;
mod config;
mod db;
mod driver;
mod model;
mod server;
mod sign;
mod static_files;

#[derive(Parser, Debug)]
#[command(name = "tinylist", author, version = "0.1.0", about = "A lightweight, high-performance file listing tool written in Rust")]
struct Cli {
    /// Data directory path
    #[arg(short, long, global = true, env = "OPENLIST_DATA_DIR", default_value = "data")]
    data_dir: PathBuf,

    /// Enable debug log level
    #[arg(long, global = true)]
    debug: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run TinyList HTTP server
    #[command(alias = "serve")]
    Server(ServerArgs),

    /// Manage admin user credentials
    Admin(AdminArgs),

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

#[derive(Args, Debug)]
struct AdminArgs {
    #[command(subcommand)]
    action: Option<AdminSubcommand>,
}

#[derive(Subcommand, Debug)]
enum AdminSubcommand {
    /// Reset admin password to a random string
    Random,
    /// Set admin password
    Set {
        password: String,
    },
    /// Show admin token
    Token,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let filter = if cli.debug {
        "tinylist=debug,tower_http=debug,axum=debug"
    } else {
        "tinylist=info,tower_http=info"
    };

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Default to 'server' if no subcommand provided
    let command = cli.command.unwrap_or(Commands::Server(ServerArgs {
        port: None,
        host: None,
    }));

    match command {
        Commands::Version => {
            println!("Version: v{}", env!("CARGO_PKG_VERSION"));
            println!("OS/Arch: {}/{}", std::env::consts::OS, std::env::consts::ARCH);
        }
        Commands::Admin(admin_args) => {
            let (config, _) = config::Config::load_or_create(&cli.data_dir)?;
            let db_path = config.resolved_db_path(&cli.data_dir);
            let pool = db::init_db(&db_path).await?;

            match admin_args.action {
                None => {
                    if let Some(admin) = db::get_admin(&pool).await? {
                        println!("Admin user's username: {}", admin.username);
                        println!("The password can only be output at the first startup, and then stored as a hash value, which cannot be reversed");
                        println!("You can reset the password with a random string by running [tinylist admin random]");
                        println!("You can also set a new password by running [tinylist admin set NEW_PASSWORD]");
                    } else {
                        eprintln!("Admin user not found in database");
                    }
                }
                Some(AdminSubcommand::Random) => {
                    let new_pwd = auth::rand_string(16);
                    db::set_admin_password(&pool, &new_pwd).await?;
                    println!("admin user has been updated:");
                    println!("username: admin");
                    println!("password: {}", new_pwd);
                }
                Some(AdminSubcommand::Set { password }) => {
                    db::set_admin_password(&pool, &password).await?;
                    println!("admin user has been updated:");
                    println!("username: admin");
                    println!("password: {}", password);
                }
                Some(AdminSubcommand::Token) => {
                    if let Some(token) = db::get_setting(&pool, "token").await? {
                        println!("Admin token: {}", token);
                    } else {
                        eprintln!("Admin token not found");
                    }
                }
            }
        }
        Commands::Server(server_args) => {
            let (mut config, config_path) = config::Config::load_or_create(&cli.data_dir)?;
            if let Some(port) = server_args.port {
                config.scheme.http_port = port;
            }
            if let Some(host) = server_args.host {
                config.scheme.address = host;
            }

            info!("loaded configuration from {:?}", config_path);

            let db_path = config.resolved_db_path(&cli.data_dir);
            info!("initializing database at {:?}", db_path);
            let pool = db::init_db(&db_path).await?;

            info!("loading storage manager...");
            let storage = driver::StorageManager::load_from_db(&pool).await?;

            server::run_server(config, pool, storage).await?;
        }
    }

    Ok(())
}
