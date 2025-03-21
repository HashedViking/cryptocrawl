use std::fs;
use std::time::Instant;
use std::path::Path;
use anyhow::{Result, Context};
use clap::Parser;
use url::Url;
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use log::{info, warn, error, debug};
use reqwest::{Client, header};
use std::collections::{HashSet, VecDeque};
use std::fs::File;
use std::io::Write;
use regex::Regex;

/// Command line arguments for the Crates.io crawler
#[derive(Parser)]
#[clap(author, version, about = "Crawl crates.io and generate a report")]
struct Args {
    /// Maximum depth to crawl
    #[clap(short, long, default_value = "2")]
    max_depth: u32,
    
    /// Follow subdomains
    #[clap(short, long)]
    follow_subdomains: bool,
    
    /// Maximum links to follow
    #[clap(short, long, default_value = "20")]
    max_links: usize,
    
    /// Output file for the crawl report
    #[clap(short, long)]
    output: Option<String>,
}

/// Represents a crawled page
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CrawledPage {
    /// URL of the page
    pub url: String,
    
    /// Size of the page content in bytes
    pub size: usize,
    
    /// When the page was crawled (Unix timestamp)
    pub timestamp: u64,
    
    /// Content type of the page
    pub content_type: Option<String>,
    
    /// HTTP status code
    pub status_code: Option<u16>,
}

/// Report that will be submitted to the manager
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CrawlReport {
    /// Task ID of the associated task
    pub task_id: String,
    
    /// Client ID that performed the crawl
    pub client_id: String,
    
    /// Domain that was crawled
    pub domain: String,
    
    /// List of crawled pages
    pub pages: Vec<CrawledPage>,
    
    /// Number of pages crawled
    pub pages_count: usize,
    
    /// Total size of all crawled pages in bytes
    pub total_size_bytes: u64,
    
    /// When the crawl started (Unix timestamp)
    pub start_time: u64,
    
    /// When the crawl ended (Unix timestamp)
    pub end_time: Option<u64>,
}

/// Ensure parent directory exists
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

/// Perform a crawl of crates.io using reqwest and simple URL extraction
async fn crawl_crates_io(max_depth: u32, follow_subdomains: bool, max_links: usize) -> Result<CrawlReport> {
    let start_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    
    // Create a unique task ID
    let task_id = Uuid::new_v4().to_string();
    
    // Create a client ID
    let client_id = format!("crates-io-crawler-{}", Uuid::new_v4());
    
    // Start URL
    let root_url = "https://crates.io/crates/";
    let root_domain = Url::parse(root_url)
        .context("Failed to parse root URL")?
        .host_str()
        .unwrap_or("crates.io")
        .to_string();
    
    // Create a new crawl report
    let mut report = CrawlReport {
        task_id,
        client_id,
        domain: root_domain.clone(),
        pages: Vec::new(),
        pages_count: 0,
        total_size_bytes: 0,
        start_time,
        end_time: None,
    };
    
    // Track visited URLs
    let mut visited_urls = std::collections::HashSet::new();
    
    // URLs to crawl (URL, depth)
    let mut urls_to_crawl = std::collections::VecDeque::new();
    urls_to_crawl.push_back((root_url.to_string(), 0));
    
    // Create HTTP client
    let client = reqwest::Client::builder()
        .user_agent("CryptoCrawl/0.1")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .context("Failed to create HTTP client")?;
    
    info!("Starting crawl of {}", root_url);
    let start = Instant::now();
    
    // Main crawl loop
    while let Some((url, depth)) = urls_to_crawl.pop_front() {
        // Check if we've reached the max links
        if report.pages_count >= max_links {
            info!("Reached maximum links: {}", max_links);
            break;
        }
        
        // Skip if already visited
        if visited_urls.contains(&url) {
            continue;
        }
        
        // Add to visited set
        visited_urls.insert(url.clone());
        
        // Fetch the page
        info!("Crawling {} (depth {})", url, depth);
        
        let response = match client.get(&url).send().await {
            Ok(response) => response,
            Err(e) => {
                warn!("Failed to fetch {}: {}", url, e);
                continue;
            }
        };
        
        // Get status code
        let status_code = response.status().as_u16();
        
        // Get content type
        let content_type = response.headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        
        // Get the HTML
        let html = match response.text().await {
            Ok(text) => text,
            Err(e) => {
                warn!("Failed to extract text from {}: {}", url, e);
                continue;
            }
        };
        
        // Calculate size
        let size = html.len();
        
        // Get timestamp
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        // Create a crawled page
        let crawled_page = CrawledPage {
            url: url.clone(),
            size,
            timestamp,
            content_type,
            status_code: Some(status_code),
        };
        
        // Add to report
        report.pages.push(crawled_page);
        report.pages_count += 1;
        report.total_size_bytes += size as u64;
        
        // Log progress
        debug!("Crawled {} ({} bytes)", url, size);
        if report.pages_count % 5 == 0 {
            info!("Crawled {} pages so far", report.pages_count);
        }
        
        // If we've reached the max depth, don't extract any more links
        if depth >= max_depth {
            continue;
        }
        
        // Extract links from the HTML
        let base_url = Url::parse(&url).unwrap_or_else(|_| Url::parse(root_url).unwrap());
        
        // Very simple link extraction regex - not robust for production
        let link_regex = Regex::new(r#"href=["']([^"']+)["']"#).unwrap();
        for cap in link_regex.captures_iter(&html) {
            let link = cap.get(1).unwrap().as_str();
            
            // Try to parse the link relative to the current page
            let parsed_url = match base_url.join(link) {
                Ok(parsed) => parsed,
                Err(_) => continue,
            };
            
            // Skip non-HTTP(S) URLs
            if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
                continue;
            }
            
            // Check if we should follow this domain
            let is_same_domain = parsed_url.host_str() == base_url.host_str();
            let root_domain_host = base_url.host_str();
            
            // Skip if not in the same domain or subdomain
            if !is_same_domain {
                // If following subdomains, check for subdomain
                if follow_subdomains {
                    let is_subdomain = parsed_url.host_str()
                        .and_then(|host| root_domain_host.map(|root| host.ends_with(&format!(".{}", root))))
                        .unwrap_or(false);
                        
                    if !is_subdomain {
                        continue;
                    }
                } else {
                    continue;
                }
            }
            
            // Add to queue if not visited
            let next_url = parsed_url.to_string();
            if !visited_urls.contains(&next_url) {
                urls_to_crawl.push_back((next_url, depth + 1));
            }
        }
    }
    
    // Complete the report
    let end_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    
    report.end_time = Some(end_time);
    
    // Log results
    let duration = start.elapsed();
    info!("Crawl completed in {:.2?}", duration);
    info!("Pages crawled: {}", report.pages_count);
    info!("Total size: {} bytes", report.total_size_bytes);
    
    Ok(report)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();
    
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    
    // Perform the crawl
    let report = crawl_crates_io(args.max_depth, args.follow_subdomains, args.max_links).await?;
    
    // Write report to file if output path provided
    if let Some(output_path) = args.output {
        info!("Writing crawl report to {}", output_path);
        
        // Ensure output directory exists
        ensure_parent_dir(Path::new(&output_path))?;
        
        // Write report to file
        let report_json = serde_json::to_string_pretty(&report)
            .context("Failed to serialize crawl report")?;
        
        fs::write(&output_path, report_json)
            .context(format!("Failed to write report to {}", output_path))?;
        
        info!("Report written to {}", output_path);
    } else {
        // Print summary to console
        println!("Crawl Report Summary:");
        println!("---------------------");
        println!("Task ID: {}", report.task_id);
        println!("Client ID: {}", report.client_id);
        println!("Domain: {}", report.domain);
        println!("Pages Crawled: {}", report.pages_count);
        println!("Total Size: {} bytes", report.total_size_bytes);
        println!("Duration: {} seconds", report.end_time.unwrap_or_default() - report.start_time);
    }
    
    Ok(())
} 