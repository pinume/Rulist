use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
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
    /// Run Rulist HTTP server
    #[command(alias = "serve")]
    Server(ServerArgs),

    /// Manage admin user credentials
    Admin(AdminArgs),

    /// Manage user passwords
    User(UserArgs),

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
    /// Show admin token
    Token,
}

#[derive(Args, Debug)]
struct UserArgs {
    #[command(subcommand)]
    action: UserSubcommand,
}

#[derive(Subcommand, Debug)]
enum UserSubcommand {
    /// Set a user's password using a hidden terminal prompt
    SetPassword { username: String },
    /// Reset a user's password to empty and disable 2FA
    ResetPassword { username: String },
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

    // Default to 'server' if no subcommand provided
    let command = cli.command.unwrap_or(Commands::Server(ServerArgs {
        port: None,
        host: None,
    }));

    match command {
        Commands::Version => {
            println!("Version: v{}", env!("CARGO_PKG_VERSION"));
            println!(
                "OS/Arch: {}/{}",
                std::env::consts::OS,
                std::env::consts::ARCH
            );
        }
        Commands::Admin(admin_args) => {
            if admin_args.action.is_none() {
                use clap::CommandFactory;
                Cli::command()
                    .find_subcommand_mut("admin")
                    .unwrap()
                    .print_help()?;
                println!();
                return Ok(());
            }
            let (config, _) = config::Config::load_or_create(&cli.data_dir)?;
            let db_path = config.resolved_db_path(&cli.data_dir);
            let pool = db::init_db(&db_path).await?;

            match admin_args.action {
                Some(AdminSubcommand::Token) => {
                    if let Some(token) = db::get_setting(&pool, "token").await? {
                        println!("Admin token: {}", token);
                    } else {
                        eprintln!("Admin token not found");
                    }
                }
                None => unreachable!(),
            }
        }
        Commands::User(args) => {
            let (config, _) = config::Config::load_or_create(&cli.data_dir)?;
            let pool = db::init_db(&config.resolved_db_path(&cli.data_dir)).await?;
            match args.action {
                UserSubcommand::SetPassword { username } => {
                    let password = rpassword::prompt_password("New password: ")?;
                    if !auth::valid_password(&password) {
                        bail!("password length must be between 8 and 128 characters");
                    }
                    if password != rpassword::prompt_password("Confirm password: ")? {
                        bail!("passwords do not match");
                    }
                    db::set_user_password(&pool, &username, &password, false).await?;
                    println!("Password set for {username}");
                }
                UserSubcommand::ResetPassword { username } => {
                    db::set_user_password(&pool, &username, "", true).await?;
                    println!("Password reset and 2FA disabled for {username}");
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
