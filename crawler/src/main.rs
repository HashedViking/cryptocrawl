mod crawler;
mod db;
mod models;
mod solana;
mod ui;

use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use log::{info, error, LevelFilter};
use std::path::PathBuf;
use crawler::Crawler;
use db::Database;
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
    #[clap(short, long, default_value = "crawler.db")]
    db_path: PathBuf,
    
    /// Log level
    #[clap(short, long, default_value = "info")]
    log_level: LevelFilter,
    
    /// Client ID (generates a new one if not provided)
    #[clap(short, long)]
    client_id: Option<String>,
    
    /// Solana keypair path
    #[clap(short = 'k', long, default_value = "wallet.json")]
    keypair_path: String,
    
    /// Solana RPC endpoint
    #[clap(short, long, default_value = "https://api.devnet.solana.com")]
    rpc_endpoint: String,
    
    /// Program ID
    #[clap(short, long, default_value = "CrawLY3R5pzRHE1b31TvhG8zX1CRkFxc1xECDZ97ihkUS")]
    program_id: String,
    
    /// Manager public key
    #[clap(short, long, default_value = "5MxUVGwsu3VAfBCwGS5sMwKyL2Vt3WvVrYLmX1fMcbZS")]
    manager_pubkey: String,
}

/// Subcommands
#[derive(Subcommand)]
enum Command {
    /// Start the crawler in UI mode
    Ui {
        /// Host to bind to
        #[clap(short, long, default_value = "127.0.0.1")]
        host: String,
        
        /// Port to bind to
        #[clap(short, long, default_value = "3000")]
        port: u16,
    },
    
    /// Crawl a single URL
    Crawl {
        /// URL to crawl
        url: String,
        
        /// Maximum depth to crawl
        #[clap(short, long, default_value = "2")]
        max_depth: u32,
        
        /// Follow subdomains
        #[clap(short, long)]
        follow_subdomains: bool,
        
        /// Maximum links to follow
        #[clap(short, long)]
        max_links: Option<usize>,
    },
    
    /// Register as a crawler with the manager
    Register,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();
    
    // Initialize logger
    env_logger::Builder::new()
        .filter_level(args.log_level)
        .init();
    
    // Generate or use provided client ID
    let client_id = args.client_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    info!("Using client ID: {}", client_id);
    
    // Initialize database
    let db = Database::new(&args.db_path)
        .context("Failed to initialize database")?;
    
    // Initialize Solana integration
    let solana = SolanaIntegration::new(
        &args.keypair_path,
        &args.rpc_endpoint,
        &args.program_id,
        &args.manager_pubkey,
    ).context("Failed to initialize Solana integration")?;
    
    // Display wallet information
    let wallet_address = solana.get_wallet_address();
    let balance = solana.get_balance()
        .context("Failed to get wallet balance")?;
    
    info!("Wallet address: {}", wallet_address);
    info!("Wallet balance: {} tokens", balance);
    
    // Initialize crawler
    let crawler = Crawler::new();
    
    // Handle command
    match args.command {
        Command::Ui { host, port } => {
            let addr = format!("{}:{}", host, port);
            info!("Starting UI server on {}", addr);
            
            // Start UI server
            ui::start_ui_server(db, crawler, solana, &addr, &client_id)
                .await
                .context("Failed to start UI server")?;
        },
        Command::Crawl { url, max_depth, follow_subdomains, max_links } => {
            // Create a task
            let task_id = Uuid::new_v4().to_string();
            let task = models::Task {
                id: task_id.clone(),
                target_url: url.clone(),
                max_depth,
                follow_subdomains,
                max_links,
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                assigned_at: Some(std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()),
                incentive_amount: 25_000_000,
            };
            
            // Save the task
            let mut db_clone = db;
            db_clone.save_task(&task).context("Failed to save task")?;
            
            // Crawl the URL
            info!("Starting crawl of {}", url);
            let mut crawler_clone = crawler;
            let result = crawler_clone.crawl(task).await.context("Failed to crawl URL")?;
            
            // Print result summary
            info!("Crawl completed: {} pages, {} bytes", result.pages_count, result.total_size);
            
            // Save the result
            db_clone.save_crawl_result(&result).context("Failed to save crawl result")?;
            
            // Submit to blockchain
            match solana.submit_crawl_data(
                &result.task_id,
                &result.domain,
                result.pages_count,
                result.total_size,
            ) {
                Ok(tx_hash) => {
                    info!("Submitted crawl data: {}", tx_hash);
                    
                    // Update with transaction hash
                    let mut updated_result = result.clone();
                    updated_result.set_transaction(tx_hash.clone());
                    
                    // Claim incentives
                    match solana.claim_incentives(&tx_hash) {
                        Ok(amount) => {
                            info!("Claimed {} tokens", amount);
                            
                            // Update with incentives
                            updated_result.set_incentives(amount);
                            db_clone.update_crawl_result(&updated_result)
                                .context("Failed to update crawl result with incentives")?;
                            
                            // Add to wallet history
                            db_clone.add_wallet_history(
                                &task_id,
                                amount,
                                &tx_hash,
                                Some(&format!("Incentive for crawling {}", url)),
                            ).context("Failed to add wallet history")?;
                        },
                        Err(e) => error!("Failed to claim incentives: {}", e),
                    }
                },
                Err(e) => error!("Failed to submit crawl data: {}", e),
            }
        },
        Command::Register => {
            // Register with manager
            solana.register_crawler(&client_id)
                .context("Failed to register with manager")?;
            
            info!("Successfully registered with manager");
        },
    }
    
    Ok(())
} 