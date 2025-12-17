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
    Daemon,

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

    /// Install an extension from git or a local package
    Install {
        /// Git URL or path to local XPI/ZIP file
        source: String,

        /// Git ref (branch, tag, commit) to build from
        #[arg(short, long)]
        r#ref: Option<String>,

        /// Custom build command
        #[arg(short, long)]
        build: Option<String>,
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
        Commands::Daemon => {
            let config = Config::load(&config_path)?;
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
            ExtensionCommands::Install {
                source,
                r#ref,
                build,
            } => {
                let path = std::path::Path::new(&source);
                let is_package = path.exists()
                    && path
                        .extension()
                        .map(|e| e == "xpi" || e == "zip")
                        .unwrap_or(false);

                if is_package {
                    cli::install_from_local_xpi(path, &config_path)?;
                } else {
                    cli::install_from_git_url(
                        &source,
                        r#ref.as_deref(),
                        build.as_deref(),
                        &config_path,
                    )?;
                }
            }
            ExtensionCommands::Uninstall { id } => {
                cli::uninstall_extension(&id, &config_path)?;
            }
        },
    }

    Ok(())
}
