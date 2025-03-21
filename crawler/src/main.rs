mod crawler;
mod db;
mod models;
mod solana;
mod ui;

use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use log::{info, error, LevelFilter};
use std::path::{PathBuf, Path};
use std::fs;
use crawler::Crawler;
use db::Database;
use solana::SolanaIntegration;
use uuid::Uuid;
use reqwest::Client;
use std::time::Duration;
use tokio::time;

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
    
    /// Path to the configuration file
    #[clap(short, long)]
    config: Option<PathBuf>,
    
    /// Manager API endpoint
    #[clap(short, long, default_value = "http://localhost:8000")]
    manager_url: String,
    
    /// Poll interval in seconds
    #[clap(short, long, default_value = "60")]
    poll_interval: u64,
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

/// Load configuration from file if provided
fn load_config(args: &mut Args) -> Result<()> {
    if let Some(config_path) = &args.config {
        info!("Loading configuration from {:?}", config_path);
        let config_str = fs::read_to_string(config_path)
            .context(format!("Failed to read config file {:?}", config_path))?;
        
        let config: serde_json::Value = serde_json::from_str(&config_str)
            .context("Failed to parse config file")?;
        
        if let Some(db_path) = config.get("db_path").and_then(|v| v.as_str()) {
            args.db_path = PathBuf::from(db_path);
        }
        
        if let Some(log_level) = config.get("log_level").and_then(|v| v.as_str()) {
            args.log_level = match log_level {
                "error" => LevelFilter::Error,
                "warn" => LevelFilter::Warn,
                "info" => LevelFilter::Info,
                "debug" => LevelFilter::Debug,
                "trace" => LevelFilter::Trace,
                _ => args.log_level,
            };
        }
        
        if let Some(keypair_path) = config.get("keypair_path").and_then(|v| v.as_str()) {
            args.keypair_path = keypair_path.to_string();
        }
        
        if let Some(rpc_endpoint) = config.get("rpc_endpoint").and_then(|v| v.as_str()) {
            args.rpc_endpoint = rpc_endpoint.to_string();
        }
        
        if let Some(program_id) = config.get("program_id").and_then(|v| v.as_str()) {
            args.program_id = program_id.to_string();
        }
        
        if let Some(manager_url) = config.get("manager_url").and_then(|v| v.as_str()) {
            args.manager_url = manager_url.to_string();
        }
        
        if let Some(poll_interval) = config.get("poll_interval").and_then(|v| v.as_u64()) {
            args.poll_interval = poll_interval;
        }
    }
    
    Ok(())
}

/// Fetch a task from the manager
async fn fetch_task(client: &Client, manager_url: &str) -> Result<Option<models::Task>> {
    info!("Requesting task from manager: {}", manager_url);
    
    let url = format!("{}/api/tasks/assign", manager_url);
    let response = client.post(&url)
        .json(&serde_json::json!({
            "crawler_id": uuid::Uuid::new_v4().to_string()
        }))
        .send()
        .await
        .context("Failed to request task from manager")?;
    
    if response.status().is_success() {
        let task: models::Task = response.json().await
            .context("Failed to parse task response")?;
        
        info!("Received task: id={}, url={}", task.id, task.target_url);
        Ok(Some(task))
    } else if response.status().as_u16() == 404 {
        info!("No tasks available from manager");
        Ok(None)
    } else {
        let status = response.status();
        let error_text = response.text().await
            .unwrap_or_else(|_| "Unknown error".to_string());
        
        error!("Error fetching task: {} - {}", status, error_text);
        Err(anyhow::anyhow!("Error fetching task: {} - {}", status, error_text))
    }
}

/// Submit crawl report to the manager
async fn submit_crawl_report(client: &Client, manager_url: &str, task_id: &str, report: &models::CrawlReport) -> Result<()> {
    info!("Submitting crawl report for task {} to manager", task_id);
    
    let url = format!("{}/api/reports/submit", manager_url);
    let response = client.post(&url)
        .json(&serde_json::json!({
            "task_id": task_id,
            "pages": report.pages,
            "transaction_signature": report.transaction_signature,
            "pages_crawled": report.pages_crawled,
            "total_size_bytes": report.total_size_bytes,
            "crawl_duration_ms": report.crawl_duration_ms
        }))
        .send()
        .await
        .context("Failed to submit report to manager")?;
    
    if response.status().is_success() {
        info!("Crawl report submitted successfully");
        Ok(())
    } else {
        let status = response.status();
        let error_text = response.text().await
            .unwrap_or_else(|_| "Unknown error".to_string());
        
        error!("Error submitting report: {} - {}", status, error_text);
        Err(anyhow::anyhow!("Error submitting report: {} - {}", status, error_text))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let mut args = Args::parse();
    
    // Load configuration if provided
    if let Err(e) = load_config(&mut args) {
        eprintln!("Warning: Failed to load configuration: {}", e);
    }
    
    // Initialize logger
    env_logger::Builder::new()
        .filter_level(args.log_level)
        .init();
    
    // Generate or use provided client ID
    let client_id = args.client_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    info!("Using client ID: {}", client_id);
    
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
    
    // Display wallet information
    let wallet_address = solana.get_wallet_address();
    let balance = solana.get_balance()
        .context("Failed to get wallet balance")?;
    
    info!("Wallet address: {}", wallet_address);
    info!("Wallet balance: {} tokens", balance);
    
    // Store manager's public key in Solana integration
    solana.set_manager_pubkey(&args.manager_pubkey);
    
    // Initialize HTTP client
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("Failed to create HTTP client")?;
    
    // Main crawler loop
    loop {
        // Fetch task from manager
        match fetch_task(&client, &args.manager_url).await {
            Ok(Some(task)) => {
                // Initialize crawler and perform the crawl
                let crawler_instance = crawler::Crawler::new(task.clone());
                
                match crawler_instance.crawl().await {
                    Ok(crawl_result) => {
                        info!("Crawl completed: {} pages crawled in {:.2?}", 
                              crawl_result.pages_count, 
                              std::time::Duration::from_secs_f64(
                                  (crawl_result.end_time.unwrap_or_default() - crawl_result.start_time) as f64
                              ));
                        
                        // Convert CrawlResult to CrawlReport
                        let report_base = crawl_result.clone().to_report();
                        
                        // Submit crawl result to Solana blockchain
                        match solana.submit_crawl_report(&task.id, &crawl_result).await {
                            Ok(signature) => {
                                // Create final crawl report with transaction signature
                                let report = models::CrawlReport {
                                    task_id: report_base.task_id,
                                    pages: report_base.pages,
                                    transaction_signature: Some(signature),
                                    pages_crawled: report_base.pages_crawled,
                                    total_size_bytes: report_base.total_size_bytes,
                                    crawl_duration_ms: report_base.crawl_duration_ms,
                                };
                                
                                // Submit report to manager
                                if let Err(e) = submit_crawl_report(&client, &args.manager_url, &task.id, &report).await {
                                    error!("Failed to submit report to manager: {}", e);
                                }
                                
                                // Save report to local database
                                if let Err(e) = db.save_crawl_report(&report) {
                                    error!("Failed to save crawl report to database: {}", e);
                                }
                            },
                            Err(e) => {
                                error!("Failed to submit crawl report to blockchain: {}", e);
                                
                                // Submit report to manager without transaction signature
                                let report = models::CrawlReport {
                                    task_id: report_base.task_id,
                                    pages: report_base.pages,
                                    transaction_signature: None,
                                    pages_crawled: report_base.pages_crawled, 
                                    total_size_bytes: report_base.total_size_bytes,
                                    crawl_duration_ms: report_base.crawl_duration_ms,
                                };
                                
                                if let Err(e) = submit_crawl_report(&client, &args.manager_url, &task.id, &report).await {
                                    error!("Failed to submit report to manager: {}", e);
                                }
                                
                                if let Err(e) = db.save_crawl_report(&report) {
                                    error!("Failed to save crawl report to database: {}", e);
                                }
                            }
                        }
                    },
                    Err(e) => {
                        error!("Crawl failed: {}", e);
                    }
                }
            },
            Ok(None) => {
                info!("No tasks available, waiting for next poll interval");
            },
            Err(e) => {
                error!("Error fetching task: {}", e);
            }
        }
        
        // Wait for the next poll interval
        info!("Waiting {} seconds before next poll", args.poll_interval);
        time::sleep(Duration::from_secs(args.poll_interval)).await;
    }
} 