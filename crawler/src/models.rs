use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// Represents a crawl task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique identifier for the task
    pub id: String,
    
    /// URL to crawl
    pub target_url: String,
    
    /// Maximum depth to crawl
    pub max_depth: u32,
    
    /// Whether to follow subdomains
    pub follow_subdomains: bool,
    
    /// Maximum number of links to follow (None for unlimited)
    pub max_links: Option<usize>,
    
    /// When the task was created (Unix timestamp)
    pub created_at: u64,
    
    /// When the task was assigned (Unix timestamp)
    pub assigned_at: Option<u64>,
    
    /// Amount of incentives for completing the task
    pub incentive_amount: u64,
}

impl Task {
    /// Create a new task with default values
    pub fn new(
        id: String,
        target_url: String,
        max_depth: u32,
        follow_subdomains: bool,
        max_links: Option<usize>,
        incentive_amount: u64,
    ) -> Self {
        Self {
            id,
            target_url,
            max_depth,
            follow_subdomains,
            max_links,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            assigned_at: None,
            incentive_amount,
        }
    }
}

/// Represents a crawled page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawledPage {
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

/// Status of a crawl operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrawlStatus {
    /// Crawl is in progress
    InProgress,
    
    /// Crawl completed successfully
    Completed,
    
    /// Crawl failed
    Failed,
    
    /// Crawl was verified by the manager
    Verified,
    
    /// Crawl was rejected by the manager
    Rejected,
}

impl fmt::Display for CrawlStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CrawlStatus::InProgress => write!(f, "InProgress"),
            CrawlStatus::Completed => write!(f, "Completed"),
            CrawlStatus::Failed => write!(f, "Failed"),
            CrawlStatus::Verified => write!(f, "Verified"),
            CrawlStatus::Rejected => write!(f, "Rejected"),
        }
    }
}

/// Represents the result of a crawl operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlResult {
    /// Task ID of the associated task
    pub task_id: String,
    
    /// Domain that was crawled
    pub domain: String,
    
    /// Status of the crawl
    pub status: CrawlStatus,
    
    /// Number of pages crawled
    pub pages_count: usize,
    
    /// List of crawled pages
    pub pages: Vec<CrawledPage>,
    
    /// Total size of all crawled pages in bytes
    pub total_size: usize,
    
    /// When the crawl started (Unix timestamp)
    pub start_time: u64,
    
    /// When the crawl ended (Unix timestamp)
    pub end_time: Option<u64>,
    
    /// Transaction hash of the submission
    pub transaction_hash: Option<String>,
    
    /// Amount of incentives received
    pub incentives_received: Option<i64>,
}

/// Represents a report that will be submitted to the manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlReport {
    /// Task ID of the associated task
    pub task_id: String,
    
    /// List of crawled pages
    pub pages: Vec<CrawledPage>,
    
    /// Transaction signature from Solana
    pub transaction_signature: Option<String>,
    
    /// Number of pages crawled
    pub pages_crawled: usize,
    
    /// Total size of all crawled pages in bytes
    pub total_size_bytes: u64,
    
    /// Duration of the crawl in milliseconds
    pub crawl_duration_ms: u64,
}

impl CrawlResult {
    /// Create a new crawl result
    pub fn new(task_id: &str, domain: &str) -> Self {
        let start_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        Self {
            task_id: task_id.to_string(),
            domain: domain.to_string(),
            status: CrawlStatus::InProgress,
            pages_count: 0,
            pages: Vec::new(),
            total_size: 0,
            start_time,
            end_time: None,
            transaction_hash: None,
            incentives_received: None,
        }
    }
    
    /// Add a crawled page to the result
    pub fn add_page(&mut self, page: CrawledPage) {
        // Update total size
        self.total_size += page.size;
        
        // Add page to list
        self.pages.push(page);
        
        // Update page count
        self.pages_count = self.pages.len();
    }
    
    /// Complete the crawl
    pub fn complete(&mut self) {
        self.status = CrawlStatus::Completed;
        self.end_time = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );
    }
    
    /// Set the crawl as failed
    pub fn set_failed(&mut self) {
        self.status = CrawlStatus::Failed;
        self.end_time = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );
    }
    
    /// Set the transaction hash
    pub fn set_transaction(&mut self, tx_hash: String) {
        self.transaction_hash = Some(tx_hash);
    }
    
    /// Set the incentives received
    pub fn set_incentives(&mut self, amount: i64) {
        self.incentives_received = Some(amount);
        self.status = CrawlStatus::Verified;
    }
    
    /// Convert to a CrawlReport
    pub fn to_report(self) -> CrawlReport {
        CrawlReport {
            task_id: self.task_id,
            pages: self.pages,
            transaction_signature: self.transaction_hash,
            pages_crawled: self.pages_count,
            total_size_bytes: self.total_size as u64,
            crawl_duration_ms: match self.end_time {
                Some(end) => (end - self.start_time) * 1000, // Convert seconds to milliseconds
                None => 0,
            },
        }
    }
} 