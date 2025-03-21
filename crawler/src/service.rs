use crate::models::{Task, CrawlResult};
use crate::crawler::Crawler;
use crate::db::Database;
use crate::solana::SolanaIntegration;
use anyhow::{Result, Context, anyhow};
use log::{info, warn, error, debug};
use reqwest::Client;
use serde_json::{json, Value};
use tokio::time::{sleep, Duration};
use tokio::sync::Mutex;
use std::sync::Arc;

/// Service to integrate crawler with the crypto manager
pub struct CrawlerService {
    /// Client ID for this crawler
    client_id: String,
    
    /// HTTP client for API requests
    client: Client,
    
    /// Database connection
    db: Arc<Mutex<Database>>,
    
    /// Solana integration
    solana: Arc<SolanaIntegration>,
    
    /// Manager API URL
    manager_url: String,
    
    /// Poll interval in seconds
    poll_interval: u64,
}

impl CrawlerService {
    /// Create a new crawler service
    pub fn new(
        client_id: String,
        manager_url: &str,
        poll_interval: u64,
        db: Database,
        solana: SolanaIntegration,
    ) -> Result<Self> {
        // Create HTTP client
        let client = Client::builder()
            .user_agent("CryptoCrawl-Service/0.1")
            .timeout(Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;
            
        // Wrap database and solana in Arc+Mutex for thread safety
        let db = Arc::new(Mutex::new(db));
        let solana = Arc::new(solana);
        
        Ok(Self {
            client_id,
            client,
            db,
            solana,
            manager_url: manager_url.to_string(),
            poll_interval,
        })
    }
    
    /// Get the client ID
    pub fn client_id(&self) -> &str {
        &self.client_id
    }
    
    /// Start the crawler service
    pub async fn start(&self) -> Result<()> {
        info!("Starting CryptoCrawl crawler service with client ID: {}", self.client_id);
        info!("Connected to manager at: {}", self.manager_url);
        
        // Register with the manager
        self.register().await?;
        
        // Run the service
        self.run().await
    }
    
    /// Start the crawler service loop
    pub async fn run(&self) -> Result<()> {
        info!("Starting crawler service with client ID {}", self.client_id);
        info!("Connecting to manager at {}", self.manager_url);
        
        // Register with the manager
        self.register().await?;
        
        // Start the main service loop
        loop {
            match self.process_next_task().await {
                Ok(true) => {
                    // Successfully processed a task, continue immediately
                    continue;
                }
                Ok(false) => {
                    // No task was available, wait before polling again
                    info!("No task available, waiting for {} seconds", self.poll_interval);
                    sleep(Duration::from_secs(self.poll_interval)).await;
                }
                Err(e) => {
                    // Error occurred, log and wait before retrying
                    error!("Error processing task: {}", e);
                    sleep(Duration::from_secs(self.poll_interval)).await;
                }
            }
        }
    }
    
    /// Process the next available task
    async fn process_next_task(&self) -> Result<bool> {
        // Fetch a task from the manager
        let task = match self.fetch_task().await? {
            Some(task) => task,
            None => return Ok(false),
        };
        
        info!("Processing task {}: {}", task.id, task.target_url);
        
        // Ensure the task exists in the database
        let mut db = self.db.lock().await;
        let db_task = db.get_task(&task.id)?;
        if db_task.is_none() {
            info!("Task {} not found in database, saving it now", task.id);
            db.save_task(&task)?;
        }
        drop(db); // Release the lock before the long-running crawl
        
        // Create crawler instance
        let task_clone = task.clone();
        let crawler = Crawler::new(task);
        
        // Execute the crawl
        info!("Starting crawl for task {}", crawler.current_task().unwrap().id);
        let crawl_result = match crawler.crawl(&task_clone).await {
            Ok(result) => result,
            Err(e) => {
                error!("Crawl failed: {}", e);
                return Err(anyhow!("Crawl failed: {}", e));
            }
        };
        
        info!("Crawl completed: {} pages, {} bytes total",
            crawl_result.pages_count, crawl_result.total_size);
        
        // Save result to database
        let mut db = self.db.lock().await;
        db.save_crawl_result(&crawl_result)?;
        
        // Convert to report and submit to manager
        self.submit_report(&crawl_result).await?;
        
        Ok(true)
    }
    
    /// Register with the manager
    pub async fn register(&self) -> Result<()> {
        info!("Registering crawler with manager");
        
        let url = format!("{}/api/crawlers/register", self.manager_url);
        let response = self.client.post(&url)
            .json(&json!({
                "client_id": self.client_id,
                "capabilities": {
                    "max_depth": 10,
                    "follow_subdomains": true,
                    "smart_mode": true
                }
            }))
            .send()
            .await
            .context("Failed to register with manager")?;
        
        if response.status().is_success() {
            info!("Successfully registered with manager");
            Ok(())
        } else {
            let status = response.status();
            let error_text = response.text().await
                .unwrap_or_else(|_| "Unknown error".to_string());
            
            warn!("Registration returned non-success status: {} - {}", status, error_text);
            
            // Don't fail on registration issues, we'll try to operate anyway
            Ok(())
        }
    }
    
    /// Fetch a task from the manager
    async fn fetch_task(&self) -> Result<Option<Task>> {
        debug!("Fetching task from manager");
        
        let url = format!("{}/api/tasks/assign", self.manager_url);
        let response = self.client.post(&url)
            .json(&json!({
                "client_id": self.client_id
            }))
            .send()
            .await
            .context("Failed to request task from manager")?;
        
        if response.status().is_success() {
            // Parse the task response
            let task_data: Value = response.json().await
                .context("Failed to parse task response")?;
            
            // Extract task fields
            let id = task_data["id"].as_str()
                .ok_or_else(|| anyhow!("Task missing id field"))?
                .to_string();
            
            let target_url = task_data["target_url"].as_str()
                .ok_or_else(|| anyhow!("Task missing target_url field"))?
                .to_string();
            
            let max_depth = task_data["max_depth"].as_u64()
                .ok_or_else(|| anyhow!("Task missing max_depth field"))? as u32;
            
            let follow_subdomains = task_data["follow_subdomains"].as_bool()
                .ok_or_else(|| anyhow!("Task missing follow_subdomains field"))?;
            
            let max_links = task_data["max_links"].as_u64().map(|v| v as usize);
            
            let incentive_amount = task_data["incentive_amount"].as_u64()
                .unwrap_or(0);
            
            // Create task
            let task = Task::new(
                id,
                target_url,
                max_depth,
                follow_subdomains,
                max_links,
                incentive_amount,
            );
            
            info!("Received task: id={}, url={}", task.id, task.target_url);
            
            // Save task to database to maintain foreign key relationship
            let mut db = self.db.lock().await;
            if let Err(e) = db.save_task(&task) {
                warn!("Failed to save task to database: {}", e);
                // Continue anyway, might work depending on DB constraints
            }
            
            Ok(Some(task))
        } else if response.status().as_u16() == 404 {
            debug!("No tasks available from manager");
            Ok(None)
        } else {
            let status = response.status();
            let error_text = response.text().await
                .unwrap_or_else(|_| "Unknown error".to_string());
            
            error!("Error fetching task: {} - {}", status, error_text);
            Err(anyhow!("Error fetching task: {} - {}", status, error_text))
        }
    }
    
    /// Submit crawl report to the manager
    async fn submit_report(&self, result: &CrawlResult) -> Result<()> {
        info!("Preparing report for task {}", result.task_id);
        
        // Create the report
        let report = result.clone().to_report();
        
        // Generate a transaction on Solana as proof of submission
        let tx_signature = match self.solana.submit_crawl_report(
            &result.task_id,
            result,
        ).await {
            Ok(sig) => {
                info!("Generated Solana transaction: {}", sig);
                Some(sig)
            },
            Err(e) => {
                warn!("Failed to generate Solana transaction: {}", e);
                None
            }
        };
        
        // Submit the report to the manager
        info!("Submitting crawl report for task {} to manager", result.task_id);
        
        let url = format!("{}/api/reports", self.manager_url);
        let response = self.client.post(&url)
            .json(&json!({
                "task_id": report.task_id,
                "client_id": self.client_id,
                "domain": result.domain,
                "pages": report.pages,
                "start_time": result.start_time,
                "end_time": result.end_time.unwrap_or_default(),
                "transaction_signature": tx_signature,
            }))
            .send()
            .await
            .context("Failed to submit report to manager")?;
        
        if response.status().is_success() {
            info!("Crawl report submitted successfully");
            
            // Parse verification result
            let verification: Value = response.json().await
                .context("Failed to parse verification response")?;
            
            let verified = verification["verified"].as_bool().unwrap_or(false);
            let score = verification["score"].as_f64().unwrap_or(0.0);
            let transaction_hash = verification["transaction_hash"].as_str()
                .unwrap_or("none").to_string();
            
            if verified {
                info!("Report verified with score {}, transaction: {}", score, transaction_hash);
            } else {
                warn!("Report was not verified, score: {}", score);
            }
            
            Ok(())
        } else {
            let status = response.status();
            let error_text = response.text().await
                .unwrap_or_else(|_| "Unknown error".to_string());
            
            error!("Error submitting report: {} - {}", status, error_text);
            Err(anyhow!("Error submitting report: {} - {}", status, error_text))
        }
    }
    
    /// Get list of currently available tasks
    pub async fn get_available_tasks(&self) -> Result<Vec<Task>> {
        debug!("Fetching available tasks from manager");
        
        let url = format!("{}/api/tasks", self.manager_url);
        let response = self.client.get(&url)
            .send()
            .await
            .context("Failed to request tasks from manager")?;
        
        if response.status().is_success() {
            // Parse the tasks
            let tasks_data: Vec<Value> = response.json().await
                .context("Failed to parse tasks response")?;
            
            let mut tasks = Vec::new();
            
            for task_data in tasks_data {
                // Extract task fields (handling possible missing fields)
                if let (
                    Some(id),
                    Some(target_url),
                    Some(max_depth),
                    Some(follow_subdomains),
                ) = (
                    task_data["id"].as_str(),
                    task_data["target_url"].as_str(),
                    task_data["max_depth"].as_u64(),
                    task_data["follow_subdomains"].as_bool(),
                ) {
                    let max_links = task_data["max_links"].as_u64().map(|v| v as usize);
                    let incentive_amount = task_data["incentive_amount"].as_u64().unwrap_or(0);
                    
                    let task = Task::new(
                        id.to_string(),
                        target_url.to_string(),
                        max_depth as u32,
                        follow_subdomains,
                        max_links,
                        incentive_amount,
                    );
                    
                    tasks.push(task);
                }
            }
            
            info!("Received {} available tasks", tasks.len());
            Ok(tasks)
        } else {
            let status = response.status();
            let error_text = response.text().await
                .unwrap_or_else(|_| "Unknown error".to_string());
            
            error!("Error fetching tasks: {} - {}", status, error_text);
            Err(anyhow!("Error fetching tasks: {} - {}", status, error_text))
        }
    }
    
    /// Process tasks using the provided crawler
    pub async fn process_tasks(&self, _crawler: Crawler) -> Result<()> {
        info!("Starting crawler service with client ID {}", self.client_id);
        info!("Connecting to manager at {}", self.manager_url);
        
        // Register with the manager
        self.register().await?;
        
        // Start the main service loop
        loop {
            match self.process_next_task().await {
                Ok(true) => {
                    // Successfully processed a task, continue immediately
                    continue;
                }
                Ok(false) => {
                    // No task was available, wait before polling again
                    info!("No task available, waiting for {} seconds", self.poll_interval);
                    sleep(Duration::from_secs(self.poll_interval)).await;
                }
                Err(e) => {
                    // Error occurred, log and wait before retrying
                    error!("Error processing task: {}", e);
                    sleep(Duration::from_secs(self.poll_interval)).await;
                }
            }
        }
    }
} 