mod crawler;
mod db;
mod models;
mod service;
mod solana;
mod ui;

use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use log::{info, error, LevelFilter};
use std::path::{PathBuf, Path};
use std::fs;
use crawler::Crawler;
use db::Database;
use service::CrawlerService;
use solana::SolanaIntegration;
use uuid::Uuid;
use reqwest::Client;
use std::time::{SystemTime, UNIX_EPOCH};

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
    #[clap(short = 'i', long)]
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
    #[clap(short = 'u', long, default_value = "http://localhost:8000")]
    manager_url: String,
    
    /// Poll interval in seconds
    #[clap(short = 't', long, default_value = "60")]
    poll_interval: u64,
}

/// Subcommands
#[derive(Subcommand)]
enum Command {
    /// Start the crawler in UI mode
    Ui {
        /// Host to bind to
        #[clap(short = 'H', long, default_value = "127.0.0.1")]
        host: String,
        
        /// Port to bind to
        #[clap(short, long, default_value = "3000")]
        port: u16,
    },
    
    /// Start the crawler service that connects to the manager
    Service {
        /// Port for the UI server
        #[clap(long, default_value = "3000")]
        server_port: u16,
        
        /// Host for the UI server
        #[clap(long, default_value = "127.0.0.1")]
        server_host: String,
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
        #[clap(short = 'l', long)]
        max_links: Option<usize>,
    },
    
    /// Crawl crates.io
    CrawlCrates {
        /// Maximum depth to crawl
        #[clap(short, long, default_value = "2")]
        max_depth: u32,
        
        /// Follow subdomains
        #[clap(short, long)]
        follow_subdomains: bool,
        
        /// Maximum links to follow
        #[clap(short = 'l', long, default_value = "20")]
        max_links: usize,
        
        /// Output file for the crawl report
        #[clap(short, long)]
        output: Option<PathBuf>,
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
#[allow(dead_code)]
async fn fetch_task(client: &Client, manager_url: &str) -> Result<Option<models::Task>> {
    info!("Requesting task from manager: {}", manager_url);
    
    let url = format!("{}/api/tasks/assign", manager_url);
    let response = client.post(&url)
        .json(&serde_json::json!({
            "client_id": Uuid::new_v4().to_string(),
        }))
        .send()
        .await
        .with_context(|| format!("Failed to connect to manager at {}", manager_url))?;
    
    if response.status().is_success() {
        let task = response.json::<models::Task>().await
            .context("Failed to parse task response")?;
        Ok(Some(task))
    } else if response.status() == 404 {
        info!("No tasks available at this time");
        Ok(None)
    } else {
        let status = response.status();
        let error_text = response.text().await
            .unwrap_or_else(|_| "Unknown error".to_string());
        Err(anyhow::anyhow!("Error from server: {} - {}", status, error_text))
    }
}

/// Submit a crawl report to the manager
#[allow(dead_code)]
async fn submit_crawl_report(client: &Client, manager_url: &str, task_id: &str, report: &models::CrawlReport) -> Result<()> {
    info!("Submitting crawl report to manager for task: {}", task_id);
    
    let url = format!("{}/api/tasks/{}/report", manager_url, task_id);
    let response = client.post(&url)
        .json(report)
        .send()
        .await
        .with_context(|| format!("Failed to connect to manager at {}", manager_url))?;
    
    if response.status().is_success() {
        info!("Crawl report submitted successfully");
        Ok(())
    } else {
        let status = response.status();
        let error_text = response.text().await
            .unwrap_or_else(|_| "Unknown error".to_string());
        Err(anyhow::anyhow!("Error from server: {} - {}", status, error_text))
    }
}

/// Main entry point
#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let mut args = Args::parse();
    
    // Load configuration from file
    load_config(&mut args).context("Failed to load configuration")?;
    
    // Set up logging
    env_logger::Builder::new()
        .filter_level(args.log_level)
        .init();
    
    info!("Starting CryptoCrawl crawler v{}", env!("CARGO_PKG_VERSION"));
    
    // Generate client ID if not provided
    let client_id = args.client_id.unwrap_or_else(|| {
        let id = Uuid::new_v4().to_string();
        info!("Generated new client ID: {}", id);
        id
    });
    
    // Ensure database directory exists
    ensure_parent_dir(&args.db_path)
        .with_context(|| format!("Failed to create directory for database at {:?}", args.db_path))?;
    
    // Connect to database
    let mut db = Database::new(&args.db_path)
        .with_context(|| format!("Failed to initialize database at {:?}", args.db_path))?;
    
    // Initialize Solana integration
    let mut solana = SolanaIntegration::new(
        &args.rpc_endpoint,
        Some(&args.keypair_path),
        &args.program_id,
    ).context("Failed to initialize Solana integration")?;
    
    // Set manager pubkey if available
    solana.set_manager_pubkey(&args.manager_pubkey);
    
    // Process command
    match args.command {
        Command::Ui { host, port } => {
            // Start the UI server
            let addr = format!("{}:{}", host, port);
            
            // Initialize crawler with no initial task
            let crawler = Crawler::default();
            
            // Start UI server
            ui::start_ui_server(db, crawler, solana, &addr, &client_id)
                .await
                .with_context(|| format!("Failed to start UI server on {}", addr))?;
        }
        
        Command::Service { server_host: _, server_port: _ } => {
            // Create crawler service
            let crawler_service = CrawlerService::new(
                client_id.clone(),
                &args.manager_url,
                args.poll_interval,
                db,
                solana,
            )
            .context("Failed to create crawler service")?;

            // Process tasks
            let crawler = Crawler::default();
            
            // Start service
            crawler_service
                .process_tasks(crawler)
                .await
                .context("Failed to process tasks")?;
        }
        
        Command::Crawl { url, max_depth, follow_subdomains, max_links } => {
            // Create crawler
            let mut crawler = Crawler::default();
            
            // Create a new task
            let task = models::Task {
                id: Uuid::new_v4().to_string(),
                target_url: url.clone(),
                max_depth,
                follow_subdomains,
                max_links,
                created_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                assigned_at: Some(SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()),
                incentive_amount: 0,
            };
            
            // Save task to database
            db.save_task(&task)
                .with_context(|| format!("Failed to save task for URL: {}", url))?;
            
            // Perform the crawl
            info!("Starting crawl for {}", url);
            let result = crawler.crawl(&task)
                .await
                .with_context(|| format!("Failed to crawl URL: {}", url))?;
            
            // Save results
            db.save_crawl_result(&result)
                .with_context(|| format!("Failed to save crawl result for task: {}", task.id))?;
            
            // Print summary
            println!("Crawl complete!");
            println!("Domain: {}", result.domain);
            println!("Pages crawled: {}", result.pages_count);
            println!("Total data size: {} bytes", result.total_size);
        }
        
        Command::CrawlCrates { max_depth, follow_subdomains, max_links, output } => {
            info!("Crawling crates.io with depth {}", max_depth);
            
            // Create a task for crawling crates.io
            let task_id = Uuid::new_v4().to_string();
            
            // Create task
            let task = models::Task::new(
                task_id,
                "https://crates.io/".to_string(),
                max_depth,
                follow_subdomains,
                Some(max_links),
                0,  // No incentive amount for direct crawls
            );
            
            // Create crawler and crawl crates.io
            let crawler = Crawler::new(task.clone());
            let result = crawler.crawl(&task).await?;
            
            // Print summary
            println!("Crawl completed!");
            println!("Pages crawled: {}", result.pages_count);
            println!("Total size: {} bytes", result.total_size);
            
            // Create report
            let report = result.to_report();
            
            // Save report to file if output is provided
            if let Some(output_path) = output {
                info!("Saving crawl report to {:?}", output_path);
                
                // Ensure parent directory exists
                ensure_parent_dir(&output_path)?;
                
                // Serialize report to JSON
                let json = serde_json::to_string_pretty(&report)?;
                
                // Write to file
                fs::write(&output_path, json)
                    .context(format!("Failed to write report to {:?}", output_path))?;
                
                println!("Report saved to {:?}", output_path);
            }
        },
        
        Command::Register => {
            info!("Registering with manager at {}", args.manager_url);
            
            // Create crawler service (only for registration)
            let service = CrawlerService::new(
                client_id.clone(),
                &args.manager_url,
                args.poll_interval,
                db,
                solana,
            ).context("Failed to initialize crawler service")?;
            
            // Register with the manager
            let client_id = service.client_id();
            if let Err(e) = service.register().await {
                error!("Failed to register with manager: {}", e);
                return Err(anyhow::anyhow!("Registration failed: {}", e));
            }
            
            println!("Successfully registered with client ID: {}", client_id);
        },
    }
    
    Ok(())
} 

// ok lets start a manager then register a crawler then let manager add a task for the crawler to crawl at @https://crates.io/  lets see how things perform, at least the manager should get the report from the crawler