mod config;
mod engine;
mod plugins;

#[cfg(test)]
mod integration_tests;

use clap::Parser;
use std::path::PathBuf;
use tracing::{error, info, Level};
use tracing_subscriber;

#[derive(Parser, Debug)]
#[command(name = "Windows Event Automation Engine")]
#[command(about = "A universal event automation system for Windows")]
#[command(version)]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Directory containing configuration files
    #[arg(short = 'd', long, value_name = "DIR")]
    config_dir: Option<PathBuf>,

    /// Run in dry-run mode (don't execute actions)
    #[arg(long)]
    dry_run: bool,

    /// Log level (debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// Show engine status and exit
    #[arg(long)]
    status: bool,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = match cli.log_level.as_str() {
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    tracing_subscriber::fmt()
        .with_max_level(log_level)
        .with_target(false)
        .init();

    info!("Windows Event Automation Engine v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let config = if let Some(config_path) = cli.config {
        info!("Loading configuration from: {:?}", config_path);
        match config::Config::load_from_file(&config_path) {
            Ok(cfg) => cfg,
            Err(e) => {
                error!("Failed to load configuration: {}", e);
                std::process::exit(1);
            }
        }
    } else if let Some(config_dir) = cli.config_dir {
        info!("Loading configuration from directory: {:?}", config_dir);
        match config::Config::load_from_dir(&config_dir) {
            Ok(cfg) => cfg,
            Err(e) => {
                error!("Failed to load configuration: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        // Try default locations
        let default_config = PathBuf::from("config.toml");
        let default_config_dir = PathBuf::from("config");

        if default_config.exists() {
            info!("Loading default configuration: config.toml");
            match config::Config::load_from_file(&default_config) {
                Ok(cfg) => cfg,
                Err(e) => {
                    error!("Failed to load configuration: {}", e);
                    std::process::exit(1);
                }
            }
        } else if default_config_dir.exists() {
            info!("Loading configuration from config/ directory");
            match config::Config::load_from_dir(&default_config_dir) {
                Ok(cfg) => cfg,
                Err(e) => {
                    error!("Failed to load configuration: {}", e);
                    std::process::exit(1);
                }
            }
        } else {
            // Use default/demo configuration
            info!("No configuration found, using default demo setup");
            create_demo_config()
        }
    };

    // Validate configuration
    if let Err(e) = config.validate() {
        error!("Configuration validation failed: {}", e);
        std::process::exit(1);
    }

    if cli.status {
        print_status(&config);
        return;
    }

    if cli.dry_run {
        info!("Running in dry-run mode (actions will not be executed)");
    }

    // Create and initialize engine
    let mut engine_instance = engine::Engine::new(config);

    if let Err(e) = engine_instance.initialize().await {
        error!("Failed to initialize engine: {}", e);
        std::process::exit(1);
    }

    let status = engine_instance.get_status();
    info!(
        "Engine running with {} plugins and {} rules",
        status.active_plugins, status.active_rules
    );

    // Setup graceful shutdown
    let (_shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);
    let mut engine_for_shutdown = engine_instance;

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
        _ = shutdown_rx.recv() => {
            info!("Received shutdown command");
        }
    }

    // Shutdown
    engine_for_shutdown.shutdown().await;
    info!("Engine stopped");
}

fn print_status(config: &config::Config) {
    println!("\n=== Engine Status ===\n");
    println!("Event Buffer Size: {}", config.engine.event_buffer_size);
    println!("Log Level: {}", config.engine.log_level);
    println!();

    println!("Sources ({}):", config.sources.len());
    for source in &config.sources {
        let status = if source.enabled { "enabled" } else { "disabled" };
        println!("  - {} ({:?}) [{}]", source.name, source.source_type, status);
    }
    println!();

    println!("Rules ({}):", config.rules.len());
    for rule in &config.rules {
        let status = if rule.enabled { "enabled" } else { "disabled" };
        println!(
            "  - {}: {:?} -> {:?} [{}]",
            rule.name, rule.trigger, rule.action, status
        );
    }
    println!();
}

fn create_demo_config() -> config::Config {
    use config::*;

    Config {
        engine: EngineConfig {
            event_buffer_size: 100,
            log_level: "info".to_string(),
        },
        sources: vec![
            SourceConfig {
                name: "test_file_watcher".to_string(),
                source_type: SourceType::FileWatcher {
                    paths: vec![PathBuf::from("./test_watch")],
                    pattern: Some("*.txt".to_string()),
                    recursive: false,
                },
                enabled: true,
            },
        ],
        rules: vec![
            RuleConfig {
                name: "text_file_created".to_string(),
                description: Some("Detect when text files are created".to_string()),
                trigger: TriggerConfig::FileCreated {
                    pattern: Some("*.txt".to_string()),
                },
                action: ActionConfig::Log {
                    message: "Text file created!".to_string(),
                    level: "info".to_string(),
                },
                enabled: true,
            },
        ],
    }
}
