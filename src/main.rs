use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use wolfpack::cli;
use wolfpack::config::Config;
use wolfpack::daemon::run_daemon;

#[derive(Parser)]
#[command(name = "wolfpack")]
#[command(about = "LibreWolf sync via Syncthing with E2E encryption")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to config file
    #[arg(short, long)]
    config: Option<std::path::PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the sync daemon
    Daemon {
        /// LibreWolf profile directory (auto-detected if not specified)
        #[arg(short, long)]
        profile: Option<std::path::PathBuf>,
    },

    /// Initialize wolfpack
    Init {
        /// Device name
        #[arg(short, long)]
        name: Option<String>,
    },

    /// Send a tab to another device
    Send {
        /// URL to send
        url: String,

        /// Target device name
        #[arg(short, long)]
        to: String,
    },

    /// List known devices
    Devices,

    /// Pair with another device
    Pair {
        /// 6-digit pairing code to join an existing session
        #[arg(short, long)]
        code: Option<String>,
    },

    /// Show sync status
    Status,

    /// Manage synced extensions
    Extension {
        #[command(subcommand)]
        command: ExtensionCommands,
    },
}

#[derive(Subcommand)]
enum ExtensionCommands {
    /// List synced extensions
    List {
        /// Show only extensions missing on this device
        #[arg(long)]
        missing: bool,
    },

    /// Install an extension from a signed XPI file
    Install {
        /// Path to XPI file
        path: std::path::PathBuf,
    },

    /// Uninstall an extension
    Uninstall {
        /// Extension ID
        id: String,
    },
}

#[tokio::main]
#[allow(clippy::too_many_lines)] // CLI entry point with command routing
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let config_path = cli.config.unwrap_or_else(Config::default_path);

    match cli.command {
        Commands::Daemon { profile } => {
            let mut config = Config::load(&config_path)?;
            if let Some(profile_path) = profile {
                config.paths.profile = Some(profile_path);
            }
            run_daemon(config).await?;
        }

        Commands::Init { name } => {
            let mut config = Config::default();
            if let Some(name) = name {
                config.device.name = name;
            }
            config.save(&config_path)?;
            println!("Initialized wolfpack");
            println!("Device ID: {}", config.device.id);
            println!("Device name: {}", config.device.name);
            println!("Config saved to: {}", config_path.display());
        }

        Commands::Send { url, to } => {
            cli::send_tab(&url, &to)?;
        }

        Commands::Devices => {
            cli::list_devices()?;
        }

        Commands::Pair { code } => {
            cli::pair_device(&config_path, code.as_deref()).await?;
        }

        Commands::Status => {
            cli::show_status()?;
        }

        Commands::Extension { command } => match command {
            ExtensionCommands::List { missing } => {
                cli::list_extensions(&config_path, missing)?;
            }
            ExtensionCommands::Install { path } => {
                cli::install_extension(&path, &config_path)?;
            }
            ExtensionCommands::Uninstall { id } => {
                cli::uninstall_extension(&id, &config_path)?;
            }
        },
    }

    Ok(())
}
