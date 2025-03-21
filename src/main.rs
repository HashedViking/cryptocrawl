use anyhow::Result;
use log::info;
use spider::website::Website;
use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use url::Url;
use tokio;
use std::error::Error;
use tokio::sync::mpsc;

// Include our modules
mod solana_integration;

use solana_integration::SolanaIntegration;

// Define a struct to store information about a crawled page
#[derive(Clone)]
struct CrawledPage {
    url: String,
    status_code: Option<u16>,
    content_type: Option<String>,
    size: usize,
}

// Define a struct to store the overall crawl report
struct CrawlReport {
    pages: Vec<CrawledPage>,
    start_time: std::time::Instant,
    target_url: String,
}

impl CrawlReport {
    fn new(target_url: String) -> Self {
        CrawlReport {
            pages: Vec::new(),
            start_time: std::time::Instant::now(),
            target_url,
        }
    }

    fn add_page(&mut self, page: CrawledPage) {
        self.pages.push(page);
    }

    fn get_stats(&self) -> (usize, std::time::Duration) {
        let count = self.pages.len();
        let duration = self.start_time.elapsed();
        (count, duration)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <target_url>", args[0]);
        std::process::exit(1);
    }

    // Get the target URL from args and clone it to avoid lifetime issues
    let target_url = args[1].clone();
    
    // Validate URL
    let url = Url::parse(&target_url)?;
    if url.scheme() != "http" && url.scheme() != "https" {
        eprintln!("Error: URL must use HTTP or HTTPS protocol");
        std::process::exit(1);
    }

    println!("Starting crawl of: {}", target_url);
    
    // Initialize the crawler
    let mut website = spider::website::Website::new(&target_url);
    
    // Initialize Solana integration
    let solana = SolanaIntegration::new(
        "https://api.devnet.solana.com", 
        None, 
        "Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS" // Example program ID
    )?;
    
    println!("Solana wallet address: {}", solana.get_wallet_address());
    
    // Subscribe to crawled pages - rx needs to be mutable
    let mut rx = website.subscribe(16).unwrap();
    
    // Target URL for the async block
    let target_url_clone = target_url.clone();
    
    // Process crawled pages
    let report_clone = tokio::spawn(async move {
        let mut crawl_report = CrawlReport::new(target_url_clone);
        
        while let Ok(page) = rx.recv().await {
            let url = page.get_url().to_string();
            
            // Extract page information
            // In the current version, get_html() returns a String
            let html = page.get_html();
            let size = html.len();
            
            // Extract HTTP status and content type
            // Note: In newer versions of spider-rs, the Page struct may not have these methods
            // directly exposed, so we're using optional values
            let status_code = None; // Would be page.get_status_code() if available
            let content_type = None; // Would be page.get_content_type() if available
            
            // Create a CrawledPage
            let crawled_page = CrawledPage {
                url,
                status_code,
                content_type,
                size,
            };
            
            // Add page to report
            crawl_report.add_page(crawled_page);
            
            // Print progress
            let (count, duration) = crawl_report.get_stats();
            println!("Crawled {} pages in {:?}", count, duration);
        }
        
        crawl_report
    });
    
    // Start the crawl
    website.crawl().await;
    
    // Unsubscribe to close the channel
    website.unsubscribe();
    
    // Get the final report
    let final_report = report_clone.await?;
    
    // Calculate totals
    let total_pages = final_report.pages.len();
    let total_size: usize = final_report.pages.iter().map(|p| p.size).sum();
    let domain = url.host_str().unwrap_or("unknown").to_string();
    
    // Print crawl statistics
    println!("\nCrawl Statistics:");
    println!("- Domain: {}", domain);
    println!("- Pages crawled: {}", total_pages);
    println!("- Total data size: {} bytes", total_size);
    println!("- Duration: {:?}", final_report.start_time.elapsed());
    
    // Submit data to Solana blockchain
    println!("\nSubmitting crawl data to Solana blockchain...");
    let tx_hash = solana.submit_crawl_data(&domain, total_pages, total_size)?;
    println!("Transaction hash: {}", tx_hash);
    
    // Claim incentives
    println!("\nClaiming incentives...");
    let incentive = solana.claim_incentives(&tx_hash)?;
    println!("Incentives claimed: {} tokens", incentive);
    
    println!("\nCrawl completed successfully!");
    Ok(())
}
