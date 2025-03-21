use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Status of a crawling task
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    Assigned,
    InProgress,
    Completed,
    Failed,
    Verified,
    Rejected,
}

/// Represents a crawling task to be assigned to clients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique identifier for the task
    pub id: String,
    /// Target URL to crawl
    pub target_url: String,
    /// Maximum depth to crawl
    pub max_depth: u32,
    /// Whether to follow subdomains
    pub follow_subdomains: bool,
    /// Maximum links to crawl
    pub max_links: Option<u32>,
    /// Creation timestamp
    pub created_at: u64,
    /// Assignment timestamp
    pub assigned_at: Option<u64>,
    /// Completion timestamp
    pub completed_at: Option<u64>,
    /// Current status
    pub status: TaskStatus,
    /// Assigned client ID (if any)
    pub assigned_to: Option<String>,
    /// Incentive amount for completion
    pub incentive_amount: u64,
}

impl Task {
    /// Create a new task
    pub fn new(id: String, target_url: String, max_depth: u32, follow_subdomains: bool, max_links: Option<u32>, incentive_amount: u64) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
            
        Task {
            id,
            target_url,
            max_depth,
            follow_subdomains,
            max_links,
            created_at: now,
            assigned_at: None,
            completed_at: None,
            status: TaskStatus::Pending,
            assigned_to: None,
            incentive_amount,
        }
    }
    
    /// Assign task to a client
    pub fn assign(&mut self, client_id: String) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
            
        self.assigned_to = Some(client_id);
        self.assigned_at = Some(now);
        self.status = TaskStatus::Assigned;
    }
    
    /// Mark task as completed
    pub fn complete(&mut self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
            
        self.completed_at = Some(now);
        self.status = TaskStatus::Completed;
    }
    
    /// Verify task completion
    pub fn verify(&mut self, is_valid: bool) {
        self.status = if is_valid {
            TaskStatus::Verified
        } else {
            TaskStatus::Rejected
        };
    }
}

/// Represents a single crawled page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawledPage {
    /// URL of the page
    pub url: String,
    /// HTTP status code
    pub status: Option<u16>,
    /// Content type of the page
    pub content_type: Option<String>,
    /// Size of the page in bytes
    pub size: usize,
    /// Timestamp when the page was crawled
    pub timestamp: u64,
}

/// Represents a complete crawl report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlReport {
    /// Task ID associated with this report
    pub task_id: String,
    /// Client ID that performed the crawl
    pub client_id: String,
    /// Domain or base URL that was crawled
    pub domain: String,
    /// Total number of pages crawled
    pub pages_count: usize,
    /// Total size of all crawled pages in bytes
    pub total_size: usize,
    /// List of all pages crawled
    pub pages: Vec<CrawledPage>,
    /// Start timestamp of the crawl
    pub start_time: u64,
    /// End timestamp of the crawl
    pub end_time: Option<u64>,
    /// Whether this report has been verified
    pub verified: bool,
    /// Verification score if analyzed
    pub verification_score: Option<f64>,
    /// LLM verification notes
    pub verification_notes: Option<String>,
} 