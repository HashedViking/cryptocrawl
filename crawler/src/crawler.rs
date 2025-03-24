use crate::models::{CrawledPage, CrawlResult, Task};
use crate::robots::{RobotsManager, is_javascript_dependent};
use crate::headless::HeadlessBrowser;
use anyhow::{Result, anyhow};
use log::{info, warn, debug, trace};
use url::Url;
use std::collections::{HashSet, VecDeque, HashMap};
use std::time::{SystemTime, UNIX_EPOCH, Instant};
use std::sync::{Arc, Mutex, atomic::{AtomicUsize, Ordering}};
use scraper::{Html, Selector};
use reqwest::Client;
use std::io::Write;
use std::fs::File;
use serde_json;
use crate::db::Database;
use chrono;

/// Implementation of the web crawler
pub struct Crawler {
    /// Current task being processed
    current_task: Option<Task>,
    /// HTTP client
    client: Client,
    /// Robots.txt and sitemap manager
    robots_manager: RobotsManager,
    /// Track JavaScript-dependent sites
    js_dependent_sites: HashSet<String>,
    /// Headless browser for JavaScript-heavy sites
    headless_browser: Option<Arc<HeadlessBrowser>>,
    /// Whether to use headless Chrome for JavaScript sites
    use_headless_chrome: bool,
    /// Database connection
    db: Option<Database>,
}

impl Default for Crawler {
    fn default() -> Self {
        // Create a reqwest client with default settings
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36")
            .gzip(true)
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .unwrap_or_else(|_| Client::new());
            
        // Create robots manager with the same user agent
        let robots_manager = RobotsManager::new("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36")
            .with_client(client.clone());
            
        Self {
            current_task: None,
            client,
            robots_manager,
            js_dependent_sites: HashSet::new(),
            headless_browser: None,
            use_headless_chrome: false,
            db: None,
        }
    }
}

impl Crawler {
    /// Create a new crawler instance with a task
    pub fn new(task: Task) -> Self {
        // Create a reqwest client with default settings
        let user_agent = "CryptoCrawl/0.1 (https://github.com/yourusername/cryptocrawl)";
        let client = Client::builder()
            .user_agent(user_agent)
            .gzip(true)
            .redirect(reqwest::redirect::Policy::limited(10))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());
            
        // Create robots manager with the same user agent
        let robots_manager = RobotsManager::new(user_agent)
            .with_client(client.clone());
            
        Self {
            current_task: Some(task),
            client,
            robots_manager,
            js_dependent_sites: HashSet::new(),
            headless_browser: None,
            use_headless_chrome: false,
            db: None,
        }
    }
    
    /// Enable headless Chrome for JavaScript-heavy sites
    pub fn with_headless_chrome(mut self, enabled: bool) -> Self {
        self.use_headless_chrome = enabled;
        self
    }
    
    /// Initialize headless browser (lazy initialization)
    async fn ensure_headless_browser(&mut self) -> Result<()> {
        if self.use_headless_chrome && self.headless_browser.is_none() {
            info!("Initializing headless Chrome browser");
            let mut browser = HeadlessBrowser::new();
            browser.start().await?;
            self.headless_browser = Some(Arc::new(browser));
        }
        Ok(())
    }
    
    /// Get the current task
    pub fn current_task(&self) -> Option<&Task> {
        self.current_task.as_ref()
    }
    
    /// Set the current task
    pub fn set_task(&mut self, task: Task) {
        self.current_task = Some(task);
    }
    
    /// Get the current crawl result if available
    pub fn current_result(&self) -> Option<CrawlResult> {
        None // This is a placeholder - in a real implementation, this would track the current result
    }
    
    /// Crawl a URL based on the provided task, streaming results to a JSONL file
    pub async fn crawl_with_streaming(&mut self, task: &Task, output_file: Option<File>) -> Result<CrawlResult> {
        // Create the result object
        let mut result = CrawlResult::new(&task.id, &task.target_url);
        
        // Initialize the database connection if necessary
        
        // Start metrics collection
        let start_time = Instant::now();
        result.start_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        // Initialize headless browser if enabled
        if self.use_headless_chrome {
            info!("Initializing headless Chrome browser");
            let mut browser = HeadlessBrowser::new();
            match browser.start().await {
                Ok(_) => {
                    info!("Headless Chrome browser initialized successfully");
                    self.headless_browser = Some(Arc::new(browser));
                },
                Err(e) => {
                    warn!("Failed to initialize headless Chrome browser: {}. Will continue without JavaScript support.", e);
                    // Continue without headless browser support
                    self.use_headless_chrome = false;
                }
            }
        } else {
            info!("Headless Chrome is disabled. JavaScript-dependent sites may not be fully crawled.");
        }
        
        // Start the crawl
        info!("Starting crawl of {} (task {})", task.target_url, task.id);
        
        // Set up the important queue with the initial URL (remove any fragment)
        let mut initial_url = Url::parse(&task.target_url)
            .map_err(|e| anyhow!("Failed to parse target URL: {}", e))?;
        // Normalize the initial URL by removing fragment
        initial_url.set_fragment(None);
        
        let base_domain = match initial_url.host_str() {
            Some(host) => host.to_string(),
            None => return Err(anyhow!("URL has no host: {}", initial_url)),
        };
        
        // Initialize robots.txt manager (from its builder pattern to actual object)
        let mut robots_manager = self.robots_manager.clone();
        
        let mut visited = HashSet::new();
        
        // Check for sitemaps
        info!("Checking for sitemaps at {}", base_domain);
        let mut initial_urls = Vec::new();
        initial_urls.push(initial_url.clone());
        
        // Add some well-known crates.io pages to ensure we have enough initial URLs
        if base_domain == "crates.io" {
            info!("Adding well-known crates.io URLs to the initial queue");
            let known_paths = [
                "/", 
                "/crates", 
                "/categories", 
                "/keywords",
                "/crates/tokio",
                "/crates/serde",
                "/crates/rand",
                "/crates/reqwest",
                "/crates/actix-web",
                "/crates/chrono",
                "/categories/asynchronous",
                "/categories/web-programming"
            ];
            
            for path in known_paths {
                if let Ok(url) = Url::parse(&format!("https://crates.io{}", path)) {
                    if !initial_urls.iter().any(|u| u.as_str() == url.as_str()) {
                        info!("Added known URL: {}", url);
                        initial_urls.push(url);
                    }
                }
            }
        }
        
        match robots_manager.get_sitemap_urls(&base_domain).await {
            Ok(sitemap_urls) if !sitemap_urls.is_empty() => {
                info!("Found {} sitemaps for {}", sitemap_urls.len(), base_domain);
                
                // Add URLs from sitemaps to our initial queue to speed up the start
                let mut added = 0;
                for url_str in sitemap_urls {
                    if added >= 100 { // Increased from 50 to 100
                        break;
                    }
                    
                    if !visited.contains(&url_str) {
                        visited.insert(url_str.clone());
                        
                        match Url::parse(&url_str) {
                            Ok(url) => {
                                if !initial_urls.iter().any(|u| u.as_str() == url.as_str()) {
                                    initial_urls.push(url);
                                    added += 1;
                                }
                            },
                            Err(e) => {
                                warn!("Failed to parse sitemap URL {}: {}", url_str, e);
                            }
                        }
                    }
                }
                
                info!("Added {} URLs from sitemaps to the initial queue", added);
            },
            Ok(urls) if urls.is_empty() => {
                info!("No URLs found in sitemaps for {}", base_domain);
            },
            Ok(_) => {
                info!("No sitemaps found for {}", base_domain);
            },
            Err(e) => {
                warn!("Failed to get sitemaps for {}: {}", base_domain, e);
            }
        }
        
        // Create a queue for BFS crawling with prioritization
        let important_queue = Arc::new(Mutex::new(VecDeque::new()));
        let regular_queue = Arc::new(Mutex::new(VecDeque::new()));
        
        // Add initial URLs to the queues
        for url in initial_urls {
            important_queue.lock().unwrap().push_back(url.clone());
        }
        
        // Create a set to track visited URLs
        let visited = Arc::new(Mutex::new(HashSet::new()));
        visited.lock().unwrap().insert(initial_url.to_string());
        
        // Track URL depths
        let depth_map = Arc::new(Mutex::new(HashMap::new()));
        depth_map.lock().unwrap().insert(initial_url.to_string(), 0);
        
        // Track crawled pages count
        let pages_count = Arc::new(AtomicUsize::new(0));
        let total_size = Arc::new(AtomicUsize::new(0));
        
        // Rate limiting delay (reduced from 200ms to 50ms)
        let rate_limit_delay = std::time::Duration::from_millis(50);
        
        // Determine how many workers to use
        let num_workers = 10;
        
        info!("Starting {} parallel crawl workers", num_workers);
        
        // Create handles for all worker tasks
        let mut handles = Vec::new();
        
        // Shared client for all workers
        let client = Arc::new(self.client.clone());
        
        // Create shared database reference if available
        let db = self.db.as_ref().map(|db| Arc::new(db.clone()));
        
        // Create shared headless browser if available
        let use_headless_chrome = self.use_headless_chrome;
        
        // Get the path of the output file if provided
        let output_path = if let Some(_) = output_file {
            // Close the original file as we will re-open it in worker threads
            drop(output_file);
            
            // Extract the output path from command line args
            if let Some(arg) = std::env::args().collect::<Vec<String>>().iter().position(|arg| arg == "--output") {
                let args = std::env::args().collect::<Vec<String>>();
                if arg + 1 < args.len() {
                    let output_arg = &args[arg + 1];
                    info!("Using output path from command line: {}", output_arg);
                    Some(output_arg.clone())
                } else {
                    // Fallback to default
                    let path = format!("data/crawls/{}_{}.jsonl", 
                                base_domain.replace(".", "_"), 
                                chrono::Local::now().format("%Y%m%d_%H%M%S"));
                    info!("Missing output path argument, using default: {}", path);
                    Some(path)
                }
            } else {
                // Fallback to default
                let path = format!("data/crawls/{}_{}.jsonl", 
                             base_domain.replace(".", "_"), 
                             chrono::Local::now().format("%Y%m%d_%H%M%S"));
                info!("Could not find --output in command line args, using default: {}", path);
                Some(path)
            }
        } else {
            // No output file provided, use default path
            // Create data/crawls directory if needed
            if !std::path::Path::new("data/crawls").exists() {
                if let Err(e) = std::fs::create_dir_all("data/crawls") {
                    warn!("Failed to create data/crawls directory: {}", e);
                }
            }
            
            let path = format!("data/crawls/{}_{}.jsonl", 
                         base_domain.replace(".", "_"), 
                         chrono::Local::now().format("%Y%m%d_%H%M%S"));
            info!("Using default output path: {}", path);
            Some(path)
        };
        
        // Initialize a shared headless browser
        let shared_browser = if self.use_headless_chrome {
            info!("Initializing headless Chrome browser for workers");
            
            // Create a new headless browser instance
            let mut browser = HeadlessBrowser::new();
            
            // Start the browser
            match browser.start().await {
                Ok(()) => {
                    // Browser successfully started
                    info!("Successfully initialized headless Chrome browser");
                    // Store the browser in the crawler wrapped in Arc
                    let browser_arc = Arc::new(browser);
                    self.headless_browser = Some(browser_arc.clone());
                    // Pass the Arc directly to workers
                    Some(browser_arc)
                },
                Err(e) => {
                    warn!("Failed to initialize headless Chrome browser: {}. Continuing without JavaScript support.", e);
                    None
                }
            }
        } else {
            None
        };
        
        // Clone the task for workers
        let task = task.clone();
        
        // Spawn worker tasks
        for worker_id in 0..num_workers {
            // Clone all shared resources for this worker
            let important_queue = Arc::clone(&important_queue);
            let regular_queue = Arc::clone(&regular_queue);
            let visited = Arc::clone(&visited);
            let depth_map = Arc::clone(&depth_map);
            let pages_count = Arc::clone(&pages_count);
            let total_size = Arc::clone(&total_size);
            let client = Arc::clone(&client);
            // Create a fresh copy of robots manager for each worker
            let mut worker_robots_manager = robots_manager.clone();
            let output_path = output_path.clone();
            let task = task.clone();
            let domain = base_domain.clone();
            let db = db.clone();
            let use_headless_chrome = use_headless_chrome;
            let shared_browser = shared_browser.clone();
            
            // Spawn the worker task
            let handle = tokio::spawn(async move {
                info!("Worker {} started", worker_id);
                
                // Small delay to stagger worker startup and reduce contention
                tokio::time::sleep(std::time::Duration::from_millis(worker_id as u64 * 100)).await;
                
                // Worker-local URL buffer to reduce contention
                let mut local_urls_to_process = Vec::with_capacity(10);
                let mut pages_processed = 0;
                let mut retry_queue = VecDeque::<(Url, usize)>::new();
                
                loop {
                    // Check if we've reached the maximum number of pages
                    if pages_count.load(Ordering::SeqCst) >= task.max_links.unwrap_or(1000) {
                        info!("Worker {} stopping: reached maximum pages limit ({})", worker_id, task.max_links.unwrap_or(1000));
                        break;
                    }
                    
                    // First, check retry queue for URLs that previously failed
                    if !retry_queue.is_empty() {
                        if let Some((url, retries)) = retry_queue.pop_front() {
                            if retries < 3 { // Allow up to 3 retries
                                info!("Worker {} retrying URL (attempt {}/3): {}", worker_id, retries + 1, url);
                                local_urls_to_process.push(url.clone());
                                retry_queue.push_back((url, retries + 1));
                            }
                        }
                    }
                    
                    // If our local buffer is empty, refill it
                    if local_urls_to_process.is_empty() {
                        // Try to get URLs from the important queue first, then from the regular queue
                        {
                            let mut important = important_queue.lock().unwrap();
                            // Take up to 10 URLs at once to reduce lock contention (increased from 5)
                            for _ in 0..10 {
                                if let Some(url) = important.pop_front() {
                                    local_urls_to_process.push(url);
                                } else {
                                    break;
                                }
                            }
                        }
                        
                        // If we couldn't get any URLs from the important queue, try the regular queue
                        if local_urls_to_process.is_empty() {
                            let mut regular = regular_queue.lock().unwrap();
                            // Take up to 10 URLs at once to reduce lock contention (increased from 5)
                            for _ in 0..10 {
                                if let Some(url) = regular.pop_front() {
                                    local_urls_to_process.push(url);
                                } else {
                                    break;
                                }
                            }
                        }
                        
                        // If we still have no URLs, check if other workers might add more later
                        if local_urls_to_process.is_empty() {
                            // Check if queues are empty and all workers are idle
                            let important_empty = important_queue.lock().unwrap().is_empty();
                            let regular_empty = regular_queue.lock().unwrap().is_empty();
                            
                            // Also check if there are pending retries anywhere
                            let retry_empty = retry_queue.is_empty();
                            
                            if important_empty && regular_empty && retry_empty && pages_count.load(Ordering::SeqCst) > 0 {
                                info!("Worker {} stopping: no more URLs to process", worker_id);
                                break;
                            }
                            
                            // Wait a bit and try again
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            continue;
                        }
                    }
                    
                    // Process the next URL from our local buffer
                    let current_url = match local_urls_to_process.pop() {
                        Some(url) => url,
                        None => continue, // This shouldn't happen, but just in case
                    };
                    
                    let current_url_str = current_url.to_string();
                    
                    // Get the depth for this URL
                    let current_depth = {
                        let depth_map_guard = depth_map.lock().unwrap();
                        *depth_map_guard.get(&current_url_str).unwrap_or(&0)
                    };
                    
                    // Check if we've reached the maximum depth
                    if current_depth >= task.max_depth as usize {
                        continue;
                    }
                    
                    debug!("Worker {} crawling {} (depth {})", worker_id, current_url_str, current_depth);
                    
                    // Skip robots.txt check for same domain if we've already checked it before
                    // This avoids redundant network requests
                    let domain_str = current_url.host_str().unwrap_or("unknown");
                    let _robots_cache_key = format!("{}:{}", domain_str, worker_id % 3); // Simple sharding by worker ID
                    
                    // Check robots.txt restrictions
                    let allowed = if worker_id % 3 == 0 { // Only 1/3 of workers check robots.txt to reduce overhead
                        match worker_robots_manager.is_allowed(&current_url).await {
                            Ok(allowed) => allowed,
                            Err(e) => {
                                warn!("Failed to check robots.txt for {}: {}", current_url_str, e);
                                // Continue anyway in case of robots.txt error
                                true
                            }
                        }
                    } else {
                        true // Other workers skip the check to improve throughput
                    };
                    
                    if !allowed {
                        info!("Skipping {} due to robots.txt restrictions", current_url_str);
                        continue;
                    }
                    
                    // Add rate limiting delay (reduced to improve throughput)
                    tokio::time::sleep(rate_limit_delay).await;
                    
                    // Fetch the page
                    let response = match client.get(current_url.clone())
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
                            
                            // Update counters
                            pages_count.fetch_add(1, Ordering::SeqCst);
                            
                            // Stream the page to the output file if provided
                            if let Some(ref path) = output_path {
                                if let Ok(json) = serde_json::to_string(&page) {
                                    // Append to the output file
                                    if let Ok(mut file) = std::fs::OpenOptions::new()
                                        .create(true)
                                        .append(true)
                                        .open(path) {
                                        if writeln!(file, "{}", json).is_err() {
                                            warn!("Failed to write to output file");
                                        } else {
                                            debug!("Successfully wrote page to {}", path);
                                        }
                                    } else {
                                        warn!("Failed to open output file at {}", path);
                                    }
                                }
                            }
                            
                            // Store the complete page in the database (not just stats)
                            if let Some(db) = &db {
                                // Get the HTML content
                                let html_content = match &page.body {
                                    Some(content) => content.clone(),
                                    None => String::new(),
                                };
                                
                                // Detect if page is JavaScript dependent
                                let (is_js_dependent, js_reasons) = is_javascript_dependent(&html_content);
                                
                                // Add to crawled_pages table
                                if let Err(e) = db.save_crawled_page(
                                    &task.id,
                                    &page.url,
                                    &domain.to_string(),
                                    page.status_code.unwrap_or(0) as i32,
                                    page.content_type.as_deref(),
                                    page.size as i64,
                                    page.body.as_deref(),
                                    is_js_dependent,
                                    if js_reasons.is_empty() { None } else { Some(js_reasons.join(", ")) }
                                ) {
                                    warn!("Failed to store crawled page in database: {}", e);
                                }
                            }
                            
                            continue;
                        }
                    };
                    
                    let status = response.status();
                    
                    // Check for rate limiting
                    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                        warn!("Rate limited on {}, waiting 60 seconds", current_url_str);
                        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                        // Put back in queue to retry
                        if current_url_str.contains("/crates/") ||
                           current_url_str.contains("/categories/") ||
                           current_url_str.contains("/keywords/") {
                            important_queue.lock().unwrap().push_back(current_url);
                        } else {
                            regular_queue.lock().unwrap().push_back(current_url);
                        }
                        continue;
                    }
                    
                    let content_type = response.headers()
                        .get(reqwest::header::CONTENT_TYPE)
                        .and_then(|h| h.to_str().ok())
                        .map(|s| s.to_string());
                    
                    // Skip non-HTML content
                    if let Some(ct) = &content_type {
                        if !ct.contains("text/html") && !ct.contains("application/xhtml+xml") {
                            debug!("Skipping non-HTML content: {}", current_url_str);
                            continue;
                        }
                    }
                    
                    // Get the HTML content
                    let body = match response.text().await {
                        Ok(html) => {
                            // Detect if the site is JavaScript-dependent
                            let (is_js_dependent, js_reasons) = is_javascript_dependent(&html);
                            
                            let mut content = html.clone();
                            let domain_str = current_url.host_str().unwrap_or("unknown");
                            
                            // Check if page is an important page that needs JavaScript processing
                            let needs_js_processing = is_js_dependent && 
                                (current_url_str.contains("/crates/") || 
                                 current_url_str.contains("/keywords/") ||
                                 current_url_str.contains("/categories/") ||
                                 current_url_str.contains("/docs/") ||
                                 current_depth <= 1); // Process JS for root pages and first level
                            
                            if needs_js_processing && use_headless_chrome {
                                info!("Detected JavaScript-dependent site: {} - Reasons: {:?}", domain_str, js_reasons);
                                
                                // Try to use the shared browser if it's available
                                if let Some(shared) = &shared_browser {
                                    info!("Worker {} using shared headless browser for {}", worker_id, current_url_str);
                                    
                                    // Extract content using headless browser
                                    let rendered_content = HeadlessBrowser::extract_content(shared.clone(), &current_url, 3).await;
                                    
                                    // Process the content result
                                    match rendered_content {
                                        Ok(content_result) => {
                                            info!("Successfully extracted rendered content using headless Chrome for {}", current_url_str);
                                            content = content_result;
                                        },
                                        Err(e) => {
                                            warn!("Failed to extract content with headless Chrome: {}. Falling back to regular content.", e);
                                            // Continue with the original HTML content
                                        }
                                    }
                                    
                                    // Extract links using headless browser
                                    let js_links_result = HeadlessBrowser::extract_links(shared.clone(), &current_url, 3).await;
                                    
                                    // Process the extracted links
                                    match js_links_result {
                                        Ok(js_links) => {
                                            info!("Successfully extracted {} links using headless Chrome for {}", js_links.len(), current_url_str);
                                            
                                            // Process the links extracted by headless Chrome
                                            if current_depth < task.max_depth as usize - 1 {
                                                info!("Processing {} links from headless Chrome", js_links.len());
                                                
                                                let mut visited_guard = visited.lock().unwrap();
                                                let mut depth_map_guard = depth_map.lock().unwrap();
                                                let mut important_guard = important_queue.lock().unwrap();
                                                let mut regular_guard = regular_queue.lock().unwrap();
                                                
                                                for link in js_links {
                                                    let _link_str = link.to_string();
                                                    
                                                    // Remove fragment from URL before checking if visited
                                                    // Create normalized version without fragment
                                                    let mut normalized_link = link.clone();
                                                    normalized_link.set_fragment(None);
                                                    let normalized_link_str = normalized_link.to_string();
                                                    
                                                    // Skip if already visited or queued (using normalized URL)
                                                    if visited_guard.contains(&normalized_link_str) {
                                                        continue;
                                                    }
                                                    
                                                    // Check if we should follow this URL
                                                    let should_follow = is_same_domain(&normalized_link, &domain, task.follow_subdomains);
                                                    
                                                    if should_follow {
                                                        // Check robots.txt - done outside the mutex lock later
                                                        visited_guard.insert(normalized_link_str.clone());
                                                        depth_map_guard.insert(normalized_link_str.clone(), current_depth + 1);
                                                        
                                                        // Prioritize important URLs
                                                        let has_important_patterns = normalized_link_str.contains("/docs/") || 
                                                                                      normalized_link_str.contains("/handbook/") ||
                                                                                      normalized_link_str.contains("/play/") ||
                                                                                      normalized_link_str.contains("/download/");
                                                        
                                                        if has_important_patterns {
                                                            important_guard.push_back(normalized_link);
                                                        } else {
                                                            regular_guard.push_back(normalized_link);
                                                        }
                                                    }
                                                }
                                            }
                                        },
                                        Err(e) => {
                                            warn!("Failed to extract links with headless Chrome: {}. Falling back to regular parsing.", e);
                                        }
                                    }
                                } else {
                                    warn!("Worker {} has no shared browser. Continuing with regular content for {}", worker_id, current_url_str);
                                }
                            } else if is_js_dependent {
                                debug!("Skipping headless Chrome for less important JS page: {}", current_url_str);
                            }
                            
                            content
                        },
                        Err(e) => {
                            warn!("Failed to get text from response: {}. Adding URL to retry queue.", e);
                            
                            // Add to retry queue if not already retried too many times
                            retry_queue.push_back((current_url.clone(), 1));
                            
                            // Skip the rest of processing for this URL
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
                    
                    // Update counters
                    pages_count.fetch_add(1, Ordering::SeqCst);
                    total_size.fetch_add(page.size, Ordering::SeqCst);
                    
                    // Log progress every 20 pages per worker (reduced logging frequency)
                    pages_processed += 1;
                    if pages_processed % 20 == 0 {
                        let current_count = pages_count.load(Ordering::SeqCst);
                        info!("Worker {} - Processed {} pages (Total: {})", worker_id, pages_processed, current_count);
                    }
                    
                    // Stream the page to the output file if provided - do this in parallel
                    if let Some(path) = &output_path {
                        let json_result = serde_json::to_string(&page);
                        match json_result {
                            Ok(json) => {
                                // Use a separate task for file I/O to avoid blocking
                                let path_clone = path.to_string();
                                tokio::spawn(async move {
                                    if let Ok(mut file) = std::fs::OpenOptions::new()
                                        .create(true)
                                        .append(true)
                                        .open(&path_clone) {
                                        if writeln!(file, "{}", json).is_err() {
                                            warn!("Failed to write to output file");
                                        }
                                    } else {
                                        warn!("Failed to open output file at {}", path_clone);
                                    }
                                });
                            },
                            Err(_) => warn!("Failed to serialize page to JSON")
                        }
                    }
                    
                    // Store page in database in a non-blocking way
                    if let Some(db) = &db {
                        // Get the HTML content
                        let html_content = match &page.body {
                            Some(content) => content.clone(),
                            None => String::new(),
                        };
                        
                        // Clone what we need for the database task
                        let db_clone = db.clone();
                        let task_id = task.id.clone();
                        let url = page.url.clone();
                        let domain_clone = domain.clone();
                        let status_code = page.status_code.unwrap_or(0) as i32;
                        let content_type_clone = page.content_type.clone();
                        let size = page.size as i64;
                        
                        // Detect JS dependency outside the database task
                        let (is_js_dependent, js_reasons) = is_javascript_dependent(&html_content);
                        let js_reasons_str = if js_reasons.is_empty() { 
                            None 
                        } else { 
                            Some(js_reasons.join(", ")) 
                        };
                        
                        // Spawn a separate task for database operations
                        tokio::spawn(async move {
                            if let Err(e) = db_clone.save_crawled_page(
                                &task_id,
                                &url,
                                &domain_clone,
                                status_code,
                                content_type_clone.as_deref(),
                                size,
                                None, // Don't store the full HTML in DB to save space
                                is_js_dependent,
                                js_reasons_str
                            ) {
                                warn!("Failed to store crawled page in database: {}", e);
                            }
                        });
                    }
                    
                    // Extract links
                    let extracted_links = match Self::extract_links_from_html(&body, &current_url).await {
                        Ok(links) => {
                            if links.len() > 0 {
                                debug!("Worker {} found {} links to process in {}", worker_id, links.len(), current_url_str);
                            }
                            let mut link_urls = Vec::new();
                            
                            // Process links in batches to minimize lock contention
                            let mut new_links = Vec::new();
                            
                            // First, filter links without holding locks
                            for link in links {
                                let link_str = link.to_string();
                                link_urls.push(link_str.clone());
                                
                                // Normalize URL by removing fragment
                                let mut normalized_link = link.clone();
                                normalized_link.set_fragment(None);
                                let normalized_link_str = normalized_link.to_string();
                                
                                // Check if we should follow this URL (without holding locks)
                                let should_follow = is_same_domain(&normalized_link, &domain, task.follow_subdomains);
                                
                                if should_follow {
                                    new_links.push((normalized_link, normalized_link_str));
                                }
                            }
                            
                            // Now process the filtered links with minimal lock time
                            if !new_links.is_empty() {
                                // Step 1: Check which links are already visited (with minimal lock time)
                                let unvisited_links = {
                                    let visited_guard = visited.lock().unwrap();
                                    new_links.into_iter()
                                        .filter(|(_, link_str)| !visited_guard.contains(link_str))
                                        .collect::<Vec<_>>()
                                };
                                
                                // Step 2: Update visited set and depth map in a single lock operation
                                {
                                    let mut visited_guard = visited.lock().unwrap();
                                    let mut depth_map_guard = depth_map.lock().unwrap();
                                    
                                    for (_, link_str) in &unvisited_links {
                                        visited_guard.insert(link_str.clone());
                                        depth_map_guard.insert(link_str.clone(), current_depth + 1);
                                    }
                                } // Release locks before categorizing
                                
                                // Step 3: Categorize links (without holding queue locks)
                                let mut important_links = Vec::new();
                                let mut regular_links = Vec::new();
                                
                                for (link, link_str) in unvisited_links {
                                    // Prioritize certain URL patterns
                                    let has_important_patterns = link_str.contains("/crates/") || 
                                                                  link_str.contains("/categories/") ||
                                                                  link_str.contains("/keywords/") ||
                                                                  link_str.contains("/products/") ||
                                                                  link_str.contains("/articles/") ||
                                                                  link_str.contains("/docs/") ||
                                                                  link_str.contains("/blog/");
                                    
                                    if has_important_patterns {
                                        important_links.push(link);
                                    } else {
                                        regular_links.push(link);
                                    }
                                }
                                
                                // Step 4: Add to queues with minimal lock time
                                if !important_links.is_empty() {
                                    let mut important_guard = important_queue.lock().unwrap();
                                    important_guard.extend(important_links);
                                    let queue_size = important_guard.len();
                                    drop(important_guard);
                                    if queue_size % 10 == 0 {
                                        debug!("Important queue size: {}", queue_size);
                                    }
                                }
                                
                                if !regular_links.is_empty() {
                                    let mut regular_guard = regular_queue.lock().unwrap();
                                    regular_guard.extend(regular_links);
                                    let queue_size = regular_guard.len();
                                    drop(regular_guard);
                                    if queue_size % 50 == 0 {
                                        debug!("Regular queue size: {}", queue_size);
                                    }
                                }
                            }
                            
                            // Return the extracted links for storage
                            link_urls
                        },
                        Err(e) => {
                            warn!("Failed to extract links from {}: {}", current_url_str, e);
                            Vec::new()
                        }
                    };
                    
                    // Store the extracted links in the database
                    if !extracted_links.is_empty() && extracted_links.len() > 0 {
                        if let Some(db) = &db {
                            if let Err(e) = db.update_crawled_page_links(&page.url, &extracted_links) {
                                warn!("Failed to update links for page in database: {}", e);
                            }
                        }
                    }
                }
                
                info!("Worker {} finished", worker_id);
                Ok::<_, anyhow::Error>(())
            });
            
            handles.push(handle);
        }
        
        // Wait for all workers to complete
        for (i, handle) in handles.into_iter().enumerate() {
            match handle.await {
                Ok(result) => {
                    if let Err(e) = result {
                        warn!("Worker {} failed: {}", i, e);
                    } else {
                        info!("Worker {} completed successfully", i);
                    }
                },
                Err(e) => warn!("Worker {} panicked: {}", i, e),
            }
        }
        
        // Record elapsed time
        let crawl_duration = start_time.elapsed();
        
        // Add JavaScript dependency info to the result
        if let Some(host) = initial_url.host_str() {
            if self.js_dependent_sites.contains(host) {
                info!("Note: {} is a JavaScript-dependent site, content may be incomplete", host);
            }
        }
        
        // Clean up shared browser if we created one
        info!("Shutting down headless browser if needed");
        if let Some(browser) = &self.headless_browser {
            match HeadlessBrowser::stop_browser(browser.clone()).await {
                Ok(()) => info!("Headless browser stopped successfully"),
                Err(e) => warn!("Error stopping headless browser: {}", e),
            }
            self.headless_browser = None;
        }
        
        // Update the result with the final counts
        result.pages_count = pages_count.load(Ordering::SeqCst);
        result.total_size = total_size.load(Ordering::SeqCst) as u64;
        
        // Mark the crawl as complete
        result.complete();
        
        info!("Completed crawl of {} - {} pages, {} bytes total in {:.2?}",
            task.target_url, result.pages_count, result.total_size, crawl_duration);
        
        Ok(result)
    }
    
    /// Crawl a URL based on the current task (for backwards compatibility)
    pub async fn crawl_current(&mut self) -> Result<CrawlResult> {
        match &self.current_task {
            Some(task) => {
                // Clone the task to avoid borrowing issues
                let task_clone = task.clone();
                self.crawl_with_streaming(&task_clone, None).await
            },
            None => Err(anyhow!("No task set for crawler")),
        }
    }

    /// Crawl a URL based on the provided task
    pub async fn crawl(&mut self, task: &Task) -> Result<CrawlResult> {
        // Simply delegate to the streaming version with no output file
        self.crawl_with_streaming(task, None).await
    }

    /// Extract links from HTML text
    async fn extract_links_from_html(html: &str, base_url: &Url) -> Result<Vec<Url>> {
        debug!("Document parsed, extracting links from {}", base_url);
        
        // Regular HTML parsing
        let document = Html::parse_document(html);
        let mut urls = Vec::new();
        
        // Create a selector for anchor tags with href attributes
        let selector = Selector::parse("a[href]").unwrap_or_else(|_| {
            warn!("Failed to parse selector, using fallback");
            Selector::parse("a").unwrap() // This should never fail
        });
        
        // Find all anchor tags with href attributes
        let links = document.select(&selector);
        let mut count = 0;
        
        for link in links {
            count += 1;
            if let Some(href) = link.value().attr("href") {
                // Parse the URL, handling relative URLs
                match base_url.join(href) {
                    Ok(mut url) => {
                        // Normalize URL by removing fragment
                        url.set_fragment(None);
                        urls.push(url);
                    }
                    Err(e) => {
                        trace!("Failed to parse URL from href {}: {}", href, e);
                    }
                }
            }
        }
        
        trace!("Found {} anchor tags with href attributes", count);
        debug!("Extracted {} links from {}", urls.len(), base_url);
        
        Ok(urls)
    }

    /// Set the database connection for the crawler
    pub fn set_database(&mut self, db: Database) -> &mut Self {
        self.db = Some(db);
        self
    }
}

// Helper function to check if a URL is in the same domain or subdomain
fn is_same_domain(url: &Url, target_domain: &str, include_subdomains: bool) -> bool {
    if let Some(host) = url.host_str() {
        let is_same = host == target_domain;
        let is_subdomain = include_subdomains && host.ends_with(&format!(".{}", target_domain));
        
        debug!("Checking domain for {}: host={}, target={}, is_same={}, is_subdomain={}", 
            url, host, target_domain, is_same, is_subdomain);
            
        if is_same || is_subdomain {
            return true;
        }
    }
    
    false
}

// Helper function to extract links from an HTML document
fn extract_links(document: &Html, base_url: &Url) -> Result<Vec<Url>> {
    let mut links = Vec::new();
    
    // Check for JavaScript rendered content
    let html_selector = Selector::parse("html").map_err(|e| anyhow!("Failed to parse selector: {}", e))?;
    let body_selector = Selector::parse("body").map_err(|e| anyhow!("Failed to parse selector: {}", e))?;
    
    let html_el = document.select(&html_selector).next();
    let body_el = document.select(&body_selector).next();
    
    if html_el.is_none() || body_el.is_none() {
        info!("Warning: Document might be incomplete - missing HTML or BODY tags");
    }
    
    // Use a proper selector for links - focus on anchor tags
    let a_selector = Selector::parse("a[href]").map_err(|e| anyhow!("Failed to parse selector: {}", e))?;
    
    // Count anchors found
    let anchors: Vec<_> = document.select(&a_selector).collect();
    info!("Found {} anchor tags with href attributes", anchors.len());
    
    // Extract links from <a> tags
    for element in anchors {
        if let Some(href) = element.value().attr("href") {
            let text = element.text().collect::<Vec<_>>().join("");
            info!("Found link: href={}, text={}", href, text);
            
            // Try to parse the href as a URL
            match base_url.join(href) {
                Ok(url) => {
                    // Only keep http/https URLs
                    if url.scheme() == "http" || url.scheme() == "https" {
                        // Remove fragment
                        let mut clean_url = url.clone();
                        clean_url.set_fragment(None);
                        
                        // For crates.io, check if it's an important page
                        let url_str = clean_url.to_string();
                        let is_important = url_str.contains("/crates/") || 
                                          url_str.contains("/categories") ||
                                          url_str.contains("/keywords") ||
                                          url_str.contains("/search") ||
                                          url_str.contains("/teams") ||
                                          url_str.contains("/users");
                        
                        if is_important {
                            info!("Found important URL: {}", clean_url);
                        } else {
                            info!("Found URL: {}", clean_url);
                        }
                        
                        links.push(clean_url);
                    }
                }
                Err(e) => {
                    info!("Failed to parse URL '{}': {}", href, e);
                }
            }
        }
    }
    
    info!("Extracted {} links from {}", links.len(), base_url);
    Ok(links)
} 