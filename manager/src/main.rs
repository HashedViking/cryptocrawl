mod api;
mod db;
mod evaluator;
mod models;
mod solana;

use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use log::{info, LevelFilter};
use std::path::{Path, PathBuf};
use std::fs;
use db::Database;
use evaluator::Evaluator;
use solana::SolanaIntegration;
use uuid::Uuid;

/// Command line arguments
#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Subcommand to run
    #[clap(subcommand)]
    command: Command,
    
    /// Database file path
    #[clap(short, long, default_value = "manager.db")]
    db_path: PathBuf,
    
    /// Log level
    #[clap(short, long, default_value = "info")]
    log_level: LevelFilter,
    
    /// Solana keypair path
    #[clap(short = 'k', long, default_value = "manager_wallet.json")]
    keypair_path: String,
    
    /// Solana RPC endpoint
    #[clap(short, long, default_value = "https://api.devnet.solana.com")]
    rpc_endpoint: String,
    
    /// Program ID
    #[clap(short, long, default_value = "CrawLY3R5pzRHE1b31TvhG8zX1CRkFxc1xECDZ97ihkUS")]
    program_id: String,
    
    /// Ollama host
    #[clap(short, long, default_value = "http://localhost:11434")]
    ollama_host: String,
    
    /// Ollama model
    #[clap(short, long, default_value = "llama3")]
    ollama_model: String,
}

/// Subcommands
#[derive(Subcommand)]
enum Command {
    /// Start the manager server
    Server {
        /// Host to bind to
        #[clap(short, long, default_value = "127.0.0.1")]
        host: String,
        
        /// Port to bind to
        #[clap(short, long, default_value = "8000")]
        port: u16,
    },
    
    /// Generate a new task
    CreateTask {
        /// Target URL
        url: String,
        
        /// Maximum depth to crawl
        #[clap(short, long, default_value = "2")]
        max_depth: u32,
        
        /// Follow subdomains
        #[clap(short, long)]
        follow_subdomains: bool,
        
        /// Maximum links to crawl
        #[clap(short, long)]
        max_links: Option<u32>,
        
        /// Incentive amount
        #[clap(short, long, default_value = "25000000")]
        incentive_amount: u64,
    },
    
    /// Get API documentation for a package
    GetAPIDocs {
        /// Package name
        package: String,
        
        /// Output file
        #[clap(short, long)]
        output: Option<String>,
    },
}

/// Ensure the directory for a file exists
fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            info!("Creating directory: {:?}", parent);
            fs::create_dir_all(parent)
                .context(format!("Failed to create directory {:?}", parent))?;
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();
    
    // Initialize logger
    env_logger::Builder::new()
        .filter_level(args.log_level)
        .init();
    
    // Ensure database directory exists
    ensure_parent_dir(&args.db_path)
        .context("Failed to ensure database directory exists")?;
    
    // Initialize database
    let db = Database::new(&args.db_path)
        .context("Failed to initialize database")?;
    
    // Ensure keypair directory exists
    let keypair_path = Path::new(&args.keypair_path);
    ensure_parent_dir(keypair_path)
        .context("Failed to ensure keypair directory exists")?;
    
    // Initialize Solana integration
    let solana = SolanaIntegration::new(
        &args.rpc_endpoint,
        Some(&args.keypair_path),
        &args.program_id,
    ).context("Failed to initialize Solana integration")?;
    
    // Initialize evaluator
    let evaluator = Evaluator::new(
        &args.ollama_host,
        &args.ollama_model,
    );
    
    // Handle command
    match args.command {
        Command::Server { host, port } => {
            let addr = format!("{}:{}", host, port);
            info!("Starting manager server on {}", addr);
            
            // Start API server
            api::start_api_server(db, evaluator, solana, &addr)
                .await
                .context("Failed to start API server")?;
        },
        Command::CreateTask { url, max_depth, follow_subdomains, max_links, incentive_amount } => {
            // Create a new task
            let task_id = Uuid::new_v4().to_string();
            let task = models::Task::new(
                task_id,
                url.clone(),
                max_depth,
                follow_subdomains,
                max_links,
                incentive_amount,
            );
            
            // Save to database
            db.create_task(&task)?;
            
            info!("Created new task: id={}, url={}", task.id, task.target_url);
        },
        Command::GetAPIDocs { package, output } => {
            // Get API docs
            info!("Fetching API documentation for package: {}", package);
            let docs = evaluator.get_api_documentation(&package).await?;
            
            match output {
                Some(path) => {
                    // Ensure output directory exists
                    let output_path = Path::new(&path);
                    ensure_parent_dir(output_path)
                        .context("Failed to ensure output directory exists")?;
                    
                    // Write to file
                    fs::write(&path, docs)
                        .context(format!("Failed to write documentation to {}", path))?;
                    
                    info!("API documentation written to {}", path);
                },
                None => {
                    // Print to console
                    println!("{}", docs);
                },
            }
        },
    }
    
    Ok(())
} 