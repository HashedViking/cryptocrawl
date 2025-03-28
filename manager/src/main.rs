mod api;
mod db;
mod evaluator;
mod models;
mod solana;

use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use log::{info, warn, error, LevelFilter};
use std::path::Path;
use std::fs;
use db::Database;
use evaluator::Evaluator;
use solana::SolanaIntegration;
use std::sync::Arc;
use tokio::sync::Mutex;
use once_cell::sync::OnceCell;

// Global config instance
static CONFIG: OnceCell<models::Config> = OnceCell::new();

/// Command line arguments
#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// Command to execute
    #[clap(subcommand)]
    command: Command,
    
    /// Log level
    #[clap(short, long, default_value = "info")]
    log_level: LevelFilter,
    
    /// Database path
    #[clap(long, default_value = "data/manager.db")]
    db_path: String,
    
    /// Solana RPC endpoint
    #[clap(long, default_value = "https://api.devnet.solana.com")]
    rpc_endpoint: String,
    
    /// Solana keypair path
    #[clap(long, default_value = "data/keypair.json")]
    keypair_path: String,
    
    /// Solana program ID
    #[clap(long, default_value = "CrawL4Lvdx5hGZ9S9xABCzAvta8P9R4W7Z4BM7nKTsKw")]
    program_id: String,
    
    /// Ollama host
    #[clap(long, default_value = "http://localhost:11434")]
    ollama_host: String,
    
    /// Ollama model
    #[clap(long, default_value = "llama3")]
    ollama_model: String,
}

/// Command variants
#[derive(Subcommand, Debug)]
enum Command {
    /// Start the manager server
    Server {
        /// Server host
        #[clap(long, default_value = "127.0.0.1")]
        host: String,
        
        /// Server port
        #[clap(long, default_value = "8000")]
        port: u16,
    },
    
    /// Create a new crawl task
    CreateTask {
        /// Target URL
        #[clap(short, long)]
        url: String,
        
        /// Maximum crawl depth
        #[clap(long, default_value = "3")]
        max_depth: u32,
        
        /// Follow subdomains
        #[clap(long)]
        follow_subdomains: bool,
        
        /// Maximum links to crawl
        #[clap(long, default_value = "100")]
        max_links: u32,
        
        /// Incentive amount
        #[clap(long, default_value = "0.1")]
        incentive_amount: f64,
    },
    
    /// Get API documentation for a package
    GetAPIDocs {
        /// Package name
        package: String,
        
        /// Output file path
        #[clap(short, long)]
        output: Option<String>,
    },
}

/// Load configuration from file
fn load_config() -> Result<()> {
    // Define config paths
    let config_paths = vec![
        "config.toml",
        "config/config.toml",
        "/etc/cryptocrawl/config.toml",
    ];
    
    // Try to load from one of the paths
    for path in config_paths {
        if Path::new(path).exists() {
            info!("Loading configuration from {}", path);
            let content = fs::read_to_string(path)
                .context(format!("Failed to read config file {}", path))?;
            
            let config: models::Config = toml::from_str(&content)
                .context(format!("Failed to parse config file {}", path))?;
            
            // Initialize global config
            CONFIG.set(config).expect("Failed to set global config");
            return Ok(());
        }
    }
    
    // Create default config
    info!("No config file found, using default configuration");
    let default_config = models::Config::default();
    
    // Ensure config directory exists
    let config_dir = Path::new("config");
    if !config_dir.exists() {
        fs::create_dir_all(config_dir)
            .context("Failed to create config directory")?;
    }
    
    // Write default config to file
    let config_path = "config/config.toml";
    let config_str = toml::to_string_pretty(&default_config)
        .context("Failed to serialize default config")?;
    
    fs::write(config_path, config_str)
        .context(format!("Failed to write default config to {}", config_path))?;
    
    // Initialize global config
    CONFIG.set(default_config).expect("Failed to set global config");
    
    Ok(())
}

/// Initialize database
fn init_db(args: &Args) -> Result<Database> {
    let _config = CONFIG.get().expect("Config not initialized");
    
    // Use command line DB path if specified, otherwise use config
    let db_path = &args.db_path;
    
    // Ensure database directory exists
    ensure_parent_dir(Path::new(db_path))
        .context("Failed to ensure database directory exists")?;
    
    // Connect to database
    info!("Connecting to database at {}", db_path);
    let db = Database::new(db_path)
        .context("Failed to initialize database")?;
    
    Ok(db)
}

/// Start the API server
async fn start_server(db: Arc<Mutex<Database>>, evaluator: Arc<Evaluator>) -> Result<()> {
    // Get config values
    let _config = CONFIG.get().expect("Config not initialized");
    
    // Create address
    let addr = format!("{}:{}", _config.server.host, _config.server.port);
    
    // Start Solana integration
    let solana = SolanaIntegration::new(
        &_config.solana.rpc_endpoint,
        Some(&_config.solana.keypair_path),
        &_config.solana.program_id,
    )?;
    
    // Ensure parent directory exists
    ensure_parent_dir(Path::new(&_config.solana.keypair_path))
        .with_context(|| "Failed to ensure parent directory exists")?;
    
    // Start API server
    info!("Starting manager server on {}", addr);
    api::start_api_server(db, evaluator, solana, &addr)
        .await
        .context("Failed to start API server")?;
    
    Ok(())
}

/// Start the manager process
async fn start_manager(_db: Arc<Mutex<Database>>, _evaluator: Arc<Evaluator>) -> Result<()> {
    // Implementation will be added later
    info!("Manager process started");
    Ok(())
}

/// Check if a shutdown signal has been received
#[allow(dead_code)]
async fn check_shutdown_signal() -> Result<bool> {
    // TODO: Implement shutdown signal checking
    // This is a placeholder that always returns false
    Ok(false)
}

/// Ensure parent directory exists
fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .context(format!("Failed to create directory: {}", parent.display()))?;
        }
    }
    
    Ok(())
}

/// Initialize the evaluator
async fn init_evaluator() -> Evaluator {
    // Get evaluator settings from config
    let config = CONFIG.get().expect("Config not initialized");
    let evaluator_config = &config.evaluator;
    
    // Create evaluator with deepseek-r1:14b as primary model
    let mut evaluator = Evaluator::new(&evaluator_config.host, "deepseek-r1:14b");
    
    // Check if Ollama service is available and find a suitable model
    match evaluator.check_service().await {
        Ok(true) => {
            info!("Ollama service is available and ready to use");
        },
        Ok(false) => {
            warn!("Ollama service is not available or no suitable model found");
            warn!("Report verification will use fallback mechanism");
        },
        Err(e) => {
            error!("Failed to check Ollama service: {}", e);
            warn!("Report verification will use fallback mechanism");
        }
    }
    
    evaluator
}

/// Main function
#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();
    
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    info!("Starting CryptoCrawl Manager");
    
    // Load configuration
    load_config()?;
    
    // Connect to database
    let db = init_db(&args)?;
    let db = Arc::new(Mutex::new(db));
    
    // Initialize evaluator
    let evaluator = init_evaluator().await;
    let evaluator = Arc::new(evaluator);
    
    // Start the server
    start_server(db.clone(), evaluator.clone()).await?;
    
    // Start the manager
    start_manager(db, evaluator).await?;
    
    info!("Manager shutdown complete");
    Ok(())
} 