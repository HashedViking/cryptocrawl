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
    
    // Configure logger
    env_logger::Builder::from_env(env_logger::Env::default())
        .filter_level(args.log_level)
        .init();
    
    // Create database
    let mut db = Database::new(&args.db_path)
        .context(format!("Failed to create database at {:?}", args.db_path))?;
    
    // Create Solana integration
    let mut solana = SolanaIntegration::new(
        &args.rpc_endpoint,
        Some(&args.keypair_path),
        &args.program_id,
    ).context("Failed to initialize Solana integration")?;
    solana.set_manager_pubkey(&args.manager_pubkey);
    
    // Save manager URL for later
    let manager_url = args.manager_url.clone();
    
    // Execute command
    match args.command {
        Command::Ui { host, port } => {
            info!("Starting crawler UI on {}:{}", host, port);
            
            // Create a default crawler with a dummy task for the UI
            let dummy_task = models::Task::new(
                "00000000-0000-0000-0000-000000000000".to_string(),
                "http://example.com".to_string(),
                1,
                false,
                Some(10),
                0,
            );
            let crawler = crawler::Crawler::new(dummy_task);
            
            // Start the UI server
            ui::start_ui_server(
                db,
                crawler,
                solana,
                &format!("{}:{}", host, port),
                &args.client_id.unwrap_or_else(|| "default".to_string()),
            ).await?;
        },
        
        Command::Service { server_port, server_host } => {
            info!("Starting crawler service");
            
            // Create crawler service
            let service = CrawlerService::new(
                args.client_id.clone(), 
                db.clone(), 
                solana.clone(), 
                manager_url.clone(),
                args.poll_interval
            );
            
            info!("Connecting to manager at: {}", manager_url);
            
            // Create a default crawler with a dummy task for the UI
            let dummy_task = models::Task::new(
                "00000000-0000-0000-0000-000000000000".to_string(),
                "http://example.com".to_string(),
                1,
                false,
                Some(10),
                0,
            );
            let crawler = crawler::Crawler::new(dummy_task);
            
            // Start the UI server in a separate task
            let ui_addr = format!("{}:{}", server_host, server_port);
            info!("Starting UI server on {}", ui_addr);
            
            // Start UI server in a separate task
            let client_id = args.client_id.clone().unwrap_or_else(|| "default".to_string());
            let ui_db = db.clone();
            let ui_solana = solana.clone();
            
            tokio::spawn(async move {
                if let Err(e) = ui::start_ui_server(
                    ui_db,
                    crawler,
                    ui_solana,
                    &ui_addr,
                    &client_id,
                ).await {
                    error!("UI server error: {}", e);
                }
            });
            
            // Run service in the main task
            service.run().await?;
        },
        
        Command::Crawl { url, max_depth, follow_subdomains, max_links } => {
            info!("Crawling {} with depth {}", url, max_depth);
            
            // Create a unique task ID for this crawl
            let task_id = Uuid::new_v4().to_string();
            
            // Create task
            let task = models::Task::new(
                task_id,
                url,
                max_depth,
                follow_subdomains,
                max_links,
                0,  // No incentive amount for direct crawls
            );
            
            // Save task to database
            db.save_task(&task)?;
            
            // Create crawler and crawl the URL
            let crawler = Crawler::new(task);
            let result = crawler.crawl().await?;
            
            // Print summary
            println!("Crawl completed!");
            println!("Pages crawled: {}", result.pages_count);
            println!("Total size: {} bytes", result.total_size);
            
            // Save result to database
            db.save_crawl_result(&result)?;
            
            info!("Crawl result saved to database");
        },
        
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
            let crawler = Crawler::new(task);
            let result = crawler.crawl().await?;
            
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
                args.client_id, 
                db, 
                solana, 
                args.manager_url,
                args.poll_interval
            );
            
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