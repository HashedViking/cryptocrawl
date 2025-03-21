use crate::models::{CrawledPage, CrawlResult, Task};
use anyhow::{Result, anyhow};
use log::{info, warn, debug};
use url::Url;
use std::collections::{HashSet, VecDeque, HashMap};
use std::time::{SystemTime, UNIX_EPOCH, Instant};
use scraper::{Html, Selector};
use reqwest::Client;

/// Implementation of the web crawler
pub struct Crawler {
    /// Current task being processed
    current_task: Task,
    /// HTTP client
    client: Client,
}

impl Crawler {
    /// Create a new crawler instance with a task
    pub fn new(task: Task) -> Self {
        // Create a reqwest client with default settings
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36")
            .gzip(true)
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .unwrap_or_else(|_| Client::new());
            
        Self {
            current_task: task,
            client,
        }
    }
    
    /// Get the current task
    pub fn current_task(&self) -> &Task {
        &self.current_task
    }
    
    /// Get the current crawl result if available
    pub fn current_result(&self) -> Option<CrawlResult> {
        None // This is a placeholder - in a real implementation, this would track the current result
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
        
        // Create a queue for BFS crawling
        let mut queue = VecDeque::new();
        queue.push_back(url.clone());
        
        // Create a set to track visited URLs
        let mut visited = HashSet::new();
        visited.insert(url.to_string());
        
        // Calculate max pages to crawl
        let max_pages = task.max_links.unwrap_or(1000);
        
        // Track URL depths
        let mut depth_map = HashMap::new();
        depth_map.insert(url.to_string(), 0);
        
        // Do the crawling directly in this task
        while let Some(current_url) = queue.pop_front() {
            let current_url_str = current_url.to_string();
            let current_depth = *depth_map.get(&current_url_str).unwrap_or(&0);
            
            // Check if we've reached the maximum depth
            if current_depth >= task.max_depth as usize {
                continue;
            }
            
            debug!("Crawling {} (depth {})", current_url_str, current_depth);
            
            // Fetch the page
            let response = match self.client.get(current_url.clone())
                .header(reqwest::header::ACCEPT, "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8")
                .header(reqwest::header::ACCEPT_LANGUAGE, "en-US,en;q=0.5")
                .send()
                .await {
                Ok(resp) => resp,
                Err(e) => {
                    warn!("Failed to fetch {}: {}", current_url_str, e);
                    // Create a crawled page with error information
                    let page = CrawledPage {
                        url: current_url_str.clone(),
                        size: 0,
                        timestamp: SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs(),
                        content_type: None,
                        status_code: None,
                        body: None,
                    };
                    
                    // Add the page to the result
                    result.add_page(page);
                    continue;
                }
            };
            
            let status = response.status();
            let content_type = response.headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|h| h.to_str().ok())
                .map(|s| s.to_string());
            
            // Get the HTML content
            let body = match response.text().await {
                Ok(html) => html,
                Err(e) => {
                    warn!("Failed to get text from {}: {}", current_url_str, e);
                    // Create a crawled page with error information
                    let page = CrawledPage {
                        url: current_url_str.clone(),
                        size: 0,
                        timestamp: SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs(),
                        content_type,
                        status_code: Some(status.as_u16()),
                        body: None,
                    };
                    
                    // Add the page to the result
                    result.add_page(page);
                    continue;
                }
            };
            
            // Create a crawled page
            let page = CrawledPage {
                url: current_url_str.clone(),
                size: body.len(),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                content_type,
                status_code: Some(status.as_u16()),
                body: Some(body.clone()),
            };
            
            // Add the page to the result
            result.add_page(page);
            
            // Log progress every 10 pages
            if result.pages_count % 10 == 0 {
                info!("Crawled {} pages so far", result.pages_count);
            }
            
            // Parse the HTML to extract links
            if task.follow_subdomains || current_depth < task.max_depth as usize - 1 {
                // Only parse links if we're continuing to crawl
                if let Ok(links) = extract_links_from_html(&body, &current_url) {
                    for link in links {
                        if is_same_domain(&link, &domain, task.follow_subdomains) {
                            let link_str = link.to_string();
                            
                            // Skip if we've already visited or queued this URL
                            if !visited.contains(&link_str) {
                                visited.insert(link_str.clone());
                                queue.push_back(link);
                                depth_map.insert(link_str, current_depth + 1);
                            }
                        }
                    }
                }
            }
            
            // Add a small delay to avoid overwhelming the server
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            
            // Check if we've reached the maximum number of pages
            if result.pages_count >= max_pages {
                debug!("Reached maximum pages limit ({})", max_pages);
                break;
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

// Helper function to extract links from HTML text
fn extract_links_from_html(html: &str, base_url: &Url) -> Result<Vec<Url>> {
    // Parse the HTML
    let document = Html::parse_document(html);
    
    // Extract links
    extract_links(&document, base_url)
}

// Helper function to extract links from an HTML document
fn extract_links(document: &Html, base_url: &Url) -> Result<Vec<Url>> {
    let selector = Selector::parse("a[href]").map_err(|e| anyhow!("Failed to parse selector: {}", e))?;
    
    let mut links = Vec::new();
    
    for element in document.select(&selector) {
        if let Some(href) = element.value().attr("href") {
            // Try to parse the href as a URL
            match base_url.join(href) {
                Ok(url) => {
                    // Only keep http/https URLs
                    if url.scheme() == "http" || url.scheme() == "https" {
                        // Remove fragment
                        let mut clean_url = url.clone();
                        clean_url.set_fragment(None);
                        links.push(clean_url);
                    }
                }
                Err(e) => {
                    debug!("Failed to parse URL '{}': {}", href, e);
                }
            }
        }
    }
    
    Ok(links)
}

// Helper function to check if a URL is in the same domain or subdomain
fn is_same_domain(url: &Url, target_domain: &str, include_subdomains: bool) -> bool {
    if let Some(host) = url.host_str() {
        if host == target_domain {
            return true;
        }
        
        if include_subdomains && host.ends_with(&format!(".{}", target_domain)) {
            return true;
        }
    }
    
    false
} 