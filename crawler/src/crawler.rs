use crate::models::{CrawledPage, CrawlResult, CrawlStatus, Task};
use anyhow::{Result, anyhow};
use log::{info, warn, error, debug};
use spider::{website::Website, http::Status, page::Page};
use tokio::sync::mpsc;
use url::Url;
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH, Instant};

/// Implementation of the web crawler
pub struct Crawler {
    /// Current task being processed
    current_task: Task,
}

impl Crawler {
    /// Create a new crawler instance with a task
    pub fn new(task: Task) -> Self {
        Self {
            current_task: task,
        }
    }
    
    /// Get the current task
    pub fn current_task(&self) -> &Task {
        &self.current_task
    }
    
    /// Crawl a URL based on the current task
    pub async fn crawl(&self) -> Result<CrawlResult> {
        let task = &self.current_task;
        
        // Parse the target URL
        let target_url = &task.target_url;
        let url = Url::parse(target_url)
            .map_err(|e| anyhow!("Invalid URL '{}': {}", target_url, e))?;
        
        // Extract domain for the result
        let domain = url.host_str()
            .ok_or_else(|| anyhow!("URL '{}' has no host", target_url))?
            .to_string();
        
        // Create a new crawl result
        let mut result = CrawlResult::new(&task.id, &domain);
        
        info!("Starting crawl of {} (task {})", target_url, task.id);
        
        // Record start time for performance measurement
        let start_time = Instant::now();
        
        // Create a channel for receiving crawled pages
        let (tx, mut rx) = mpsc::channel(100);
        
        // Create a set to track visited URLs and avoid duplicates
        let mut visited_urls = HashSet::new();
        visited_urls.insert(target_url.to_string());
        
        // Create and configure a website crawler
        let mut website = Website::new(target_url);
        
        // Configure based on task parameters
        website.config.respect_robots_txt = true;
        website.config.subdomains = task.follow_subdomains;
        website.config.max_depth = task.max_depth as usize;
        website.config.max_pages = task.max_links.unwrap_or(1000) as usize;
        website.config.delay = 200; // 200ms delay between requests
        
        // Clone transmitter for the subscription
        let tx_clone = tx.clone();
        
        // Subscribe to receive crawled pages and process them in a separate task
        let _subscription = website.subscribe(move |page: Page| {
            let tx = tx_clone.clone();
            tokio::spawn(async move {
                // Send the page to our channel
                if let Err(e) = tx.send(page).await {
                    error!("Failed to send page through channel: {}", e);
                }
            });
        });
        
        // Start the crawl in a separate task
        tokio::spawn(async move {
            // This initiates the crawl and returns immediately
            let _ = website.crawl();
        });
        
        // Process received pages
        let mut pages_count = 0;
        let max_pages = task.max_links.unwrap_or(1000);
        
        while let Some(page) = rx.recv().await {
            // Check if we've reached the maximum number of pages
            if pages_count >= max_pages {
                debug!("Reached maximum pages limit ({})", max_pages);
                break;
            }
            
            // Get the URL of the page
            let page_url = page.get_url().to_string();
            
            // Skip if we've already processed this URL
            if visited_urls.contains(&page_url) {
                continue;
            }
            
            // Add the URL to the visited set
            visited_urls.insert(page_url.clone());
            
            // Extract page information
            let status_code = match page.get_http_status() {
                Status::Ok => Some(200),
                Status::NotFound => Some(404),
                Status::Forbidden => Some(403),
                Status::InternalServerError => Some(500),
                Status::Unknown => None,
            };
            
            // Get content type
            let content_type = page.get_content_type().map(String::from);
            
            // Get HTML content
            let html = match page.get_html() {
                Some(content) => content,
                None => {
                    warn!("No HTML content for {}", page_url);
                    String::new()
                }
            };
            
            // Calculate content size
            let size = html.len();
            
            // Get current timestamp
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            
            // Create a crawled page
            let crawled_page = CrawledPage {
                url: page_url.clone(),
                size,
                timestamp,
                content_type,
                status_code,
            };
            
            // Add the page to the result
            result.add_page(crawled_page);
            
            // Increment page count
            pages_count += 1;
            
            // Log progress
            debug!("Crawled {} ({} bytes)", page_url, size);
            
            // Log progress every 10 pages
            if pages_count % 10 == 0 {
                info!("Crawled {} pages so far", pages_count);
            }
        }
        
        // Record elapsed time
        let crawl_duration = start_time.elapsed();
        
        // Mark the crawl as complete
        result.complete();
        
        info!("Completed crawl of {} - {} pages, {} bytes total in {:.2?}",
            target_url, result.pages_count, result.total_size, crawl_duration);
        
        Ok(result)
    }
} 