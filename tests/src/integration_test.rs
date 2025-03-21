use std::process::{Command, Child};
use std::time::Duration;
use std::thread;
use std::path::Path;
use std::fs;
use reqwest::Client;
use serde_json::{json, Value};
use rusqlite::{Connection, Result as SqliteResult};
use anyhow::{Result, Context, anyhow};

// Test configuration
const MANAGER_HOST: &str = "127.0.0.1";
const MANAGER_PORT: u16 = 8000;
const CRAWLER_HOST: &str = "127.0.0.1";
const CRAWLER_PORT: u16 = 3001;
const TEST_URL: &str = "https://crates.io/";
const MAX_WAIT_TIME_SECS: u64 = 180; // 3 minutes max wait time for test

/// Main integration test function
async fn run_integration_test() -> Result<()> {
    println!("Starting integration test...");
    
    // Clean up any existing database files from previous test runs
    cleanup_test_environment()?;
    
    // Start the manager service
    println!("Starting manager service...");
    let manager_process = start_manager()?;
    
    // Give manager time to initialize - increased to 15 seconds
    println!("Waiting for manager to initialize...");
    thread::sleep(Duration::from_secs(15));
    
    // Start the crawler service
    println!("Starting crawler service...");
    let crawler_process = start_crawler()?;
    
    // Give crawler time to initialize - increased to 10 seconds
    println!("Waiting for crawler to initialize...");
    thread::sleep(Duration::from_secs(10));
    
    // Create HTTP client for API calls
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("Failed to create HTTP client")?;
    
    // Register crawler with the manager with retry logic
    println!("Registering crawler with manager...");
    let client_id = register_crawler_with_retry(&client, 5).await?;
    println!("Crawler registered with client ID: {}", client_id);
    
    // Create a crawl task
    println!("Creating crawl task for URL: {}", TEST_URL);
    let task_id = create_task(&client, TEST_URL).await?;
    println!("Task created with ID: {}", task_id);
    
    // Wait for the task to be assigned and completed
    println!("Waiting for task to be completed...");
    let success = wait_for_task_completion(&client, &task_id).await?;
    
    if success {
        println!("Task completed successfully!");
    } else {
        return Err(anyhow!("Task did not complete within the expected time"));
    }
    
    // Verify database has crawl data
    println!("Verifying crawl data in database...");
    verify_crawl_data()?;
    
    // Clean up processes
    println!("Shutting down services...");
    if let Err(e) = terminate_process(crawler_process) {
        println!("Warning: Failed to terminate crawler process: {}", e);
    }
    
    if let Err(e) = terminate_process(manager_process) {
        println!("Warning: Failed to terminate manager process: {}", e);
    }
    
    println!("Integration test completed successfully!");
    Ok(())
}

/// Clean up test environment by removing database files if possible
fn cleanup_test_environment() -> Result<()> {
    // Make sure the data directory exists
    let data_dir = Path::new("../data");
    if !data_dir.exists() {
        fs::create_dir_all(data_dir).context("Failed to create data directory")?;
    }

    // Try to remove manager database
    if Path::new("../data/manager.db").exists() {
        match fs::remove_file("../data/manager.db") {
            Ok(_) => println!("Removed existing manager database"),
            Err(e) => println!("Note: Could not remove manager database: {}", e),
        }
    }
    
    // Try to remove crawler database
    if Path::new("../data/crawler.db").exists() {
        match fs::remove_file("../data/crawler.db") {
            Ok(_) => println!("Removed existing crawler database"),
            Err(e) => println!("Note: Could not remove crawler database: {}", e),
        }
    }
    
    // Continue regardless of whether the files could be deleted
    Ok(())
}

/// Start the manager service
fn start_manager() -> Result<Child> {
    // Get the absolute path to the workspace root
    let current_dir = std::env::current_dir().context("Failed to get current directory")?;
    
    // Navigate up from tests/src or tests to the workspace root
    let workspace_root = if current_dir.ends_with("src") {
        current_dir.parent().context("Failed to get parent")?.parent().context("Failed to get workspace root")?
    } else if current_dir.ends_with("tests") {
        current_dir.parent().context("Failed to get workspace root")?
    } else {
        &current_dir // Already at workspace root
    };
    
    println!("Starting manager from: {}", workspace_root.display());
    
    // Start the manager service
    let process = Command::new("cargo")
        .current_dir(workspace_root)
        .args([
            "run", 
            "--bin", 
            "cryptocrawl-manager", 
            "--", 
            "server", 
            "--host", 
            MANAGER_HOST, 
            "--port", 
            &MANAGER_PORT.to_string()
        ])
        .spawn()
        .context("Failed to start manager process")?;
    
    Ok(process)
}

/// Start the crawler service
fn start_crawler() -> Result<Child> {
    // Get the absolute path to the workspace root
    let current_dir = std::env::current_dir().context("Failed to get current directory")?;
    
    // Navigate up from tests/src or tests to the workspace root
    let workspace_root = if current_dir.ends_with("src") {
        current_dir.parent().context("Failed to get parent")?.parent().context("Failed to get workspace root")?
    } else if current_dir.ends_with("tests") {
        current_dir.parent().context("Failed to get workspace root")?
    } else {
        &current_dir // Already at workspace root
    };
    
    println!("Starting crawler from: {}", workspace_root.display());
    
    // Start the crawler service
    let process = Command::new("cargo")
        .current_dir(workspace_root)
        .args([
            "run", 
            "--bin", 
            "cryptocrawl-crawler", 
            "--", 
            "service", 
            "--server-host", 
            CRAWLER_HOST, 
            "--server-port", 
            &CRAWLER_PORT.to_string()
        ])
        .spawn()
        .context("Failed to start crawler process")?;
    
    Ok(process)
}

/// Register the crawler with the manager with retry logic
async fn register_crawler_with_retry(client: &Client, max_attempts: usize) -> Result<String> {
    // Generate a client ID
    let client_id = format!("test-crawler-{}", uuid::Uuid::new_v4());
    
    // Try multiple times to register
    for attempt in 1..=max_attempts {
        println!("Registration attempt {} of {}", attempt, max_attempts);
        
        // Try to register
        match register_crawler_internal(client, &client_id).await {
            Ok(()) => {
                println!("Registration successful on attempt {}", attempt);
                return Ok(client_id);
            },
            Err(e) => {
                if attempt < max_attempts {
                    println!("Registration failed: {}. Retrying in 5 seconds...", e);
                    thread::sleep(Duration::from_secs(5));
                } else {
                    return Err(anyhow!("Failed to register after {} attempts: {}", max_attempts, e));
                }
            }
        }
    }
    
    // This should never be reached due to the return in the loop
    Err(anyhow!("Failed to register crawler"))
}

/// Internal function to handle the registration request
async fn register_crawler_internal(client: &Client, client_id: &str) -> Result<()> {
    // Send registration request
    let url = format!("http://{}:{}/api/crawlers/register", MANAGER_HOST, MANAGER_PORT);
    let response = client.post(&url)
        .json(&json!({
            "client_id": client_id,
            "capabilities": {
                "max_depth": 10,
                "follow_subdomains": true,
                "smart_mode": true
            }
        }))
        .send()
        .await
        .context("Failed to connect to manager")?;
    
    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(anyhow!("Failed to register crawler: {} - {}", status, error_text));
    }
    
    Ok(())
}

/// Create a crawl task
async fn create_task(client: &Client, url: &str) -> Result<String> {
    // Add retry logic similar to registration
    for attempt in 1..=5 {
        println!("Task creation attempt {} of 5", attempt);
        
        match create_task_internal(client, url).await {
            Ok(task_id) => {
                println!("Task creation successful on attempt {}", attempt);
                return Ok(task_id);
            },
            Err(e) => {
                if attempt < 5 {
                    println!("Task creation failed: {}. Retrying in 5 seconds...", e);
                    thread::sleep(Duration::from_secs(5));
                } else {
                    return Err(anyhow!("Failed to create task after 5 attempts: {}", e));
                }
            }
        }
    }
    
    Err(anyhow!("Failed to create task"))
}

/// Internal function to handle the task creation request
async fn create_task_internal(client: &Client, url: &str) -> Result<String> {
    // Send task creation request
    let request_url = format!("http://{}:{}/api/tasks", MANAGER_HOST, MANAGER_PORT);
    let response = client.post(&request_url)
        .json(&json!({
            "target_url": url,
            "max_depth": 2,
            "follow_subdomains": false,
            "max_links": 50,
            "incentive_amount": 100000
        }))
        .send()
        .await
        .context("Failed to connect to manager")?;
    
    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(anyhow!("Failed to create task: {} - {}", status, error_text));
    }
    
    // Parse the response to get the task ID
    let task_data: Value = response.json().await.context("Failed to parse task response")?;
    let task_id = task_data["id"].as_str()
        .ok_or_else(|| anyhow!("Task ID not found in response"))?
        .to_string();
    
    Ok(task_id)
}

/// Wait for a task to be completed
async fn wait_for_task_completion(client: &Client, task_id: &str) -> Result<bool> {
    let start_time = std::time::Instant::now();
    
    while start_time.elapsed().as_secs() < MAX_WAIT_TIME_SECS {
        // Check task status
        let url = format!("http://{}:{}/api/tasks/{}", MANAGER_HOST, MANAGER_PORT, task_id);
        let response = client.get(&url)
            .send()
            .await
            .context("Failed to get task status")?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow!("Failed to check task status: {} - {}", status, error_text));
        }
        
        // Parse the response
        let task_data: Value = response.json().await.context("Failed to parse task response")?;
        let status = task_data["status"].as_str().unwrap_or("Unknown");
        
        println!("Task status: {}", status);
        
        // Check if the task is completed
        if status == "Completed" || status == "Verified" {
            return Ok(true);
        }
        
        // Wait before checking again
        thread::sleep(Duration::from_secs(5));
    }
    
    // Timed out waiting for task completion
    Ok(false)
}

/// Verify that crawl data exists in the database
fn verify_crawl_data() -> Result<()> {
    // Check manager database - with corrected paths
    let manager_conn = Connection::open("../data/manager.db").context("Failed to open manager database")?;
    let task_count: i64 = get_count(&manager_conn, "SELECT COUNT(*) FROM tasks")?;
    
    println!("Manager database: {} tasks", task_count);
    
    if task_count == 0 {
        return Err(anyhow!("No tasks found in manager database"));
    }
    
    // Check crawler database - with corrected paths
    let crawler_conn = Connection::open("../data/crawler.db").context("Failed to open crawler database")?;
    let result_count: i64 = get_count(&crawler_conn, "SELECT COUNT(*) FROM crawl_results")?;
    
    println!("Crawler database: {} results", result_count);
    
    if result_count == 0 {
        return Err(anyhow!("No crawl results found in crawler database"));
    }
    
    // Verify that the data in both databases is related
    let task_id: String = manager_conn.query_row(
        "SELECT id FROM tasks LIMIT 1", 
        [], 
        |row| row.get(0)
    ).context("Failed to get task ID from manager database")?;
    
    let crawler_task_id: String = crawler_conn.query_row(
        "SELECT task_id FROM crawl_results LIMIT 1", 
        [], 
        |row| row.get(0)
    ).context("Failed to get task ID from crawler database")?;
    
    if task_id != crawler_task_id {
        println!("Warning: Task IDs don't match between databases. Manager: {}, Crawler: {}", task_id, crawler_task_id);
    } else {
        println!("✅ Task IDs match between databases: {}", task_id);
    }
    
    Ok(())
}

/// Get count from a SQL query
fn get_count(conn: &Connection, query: &str) -> SqliteResult<i64> {
    conn.query_row(query, [], |row| row.get(0))
}

/// Terminate a process
fn terminate_process(mut process: Child) -> Result<()> {
    #[cfg(target_family = "unix")]
    {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;
        
        let pid = Pid::from_raw(process.id() as i32);
        kill(pid, Signal::SIGTERM).context("Failed to send SIGTERM")?;
    }
    
    #[cfg(target_family = "windows")]
    {
        // Windows implementation
        process.kill().context("Failed to kill process")?;
    }
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Setup logging to help debug issues
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    
    match run_integration_test().await {
        Ok(_) => {
            println!("✅ Integration test passed successfully!");
            Ok(())
        },
        Err(e) => {
            println!("❌ Integration test failed: {}", e);
            println!("Cleaning up any remaining processes...");
            // Additional cleanup if needed
            Err(e)
        }
    }
} 