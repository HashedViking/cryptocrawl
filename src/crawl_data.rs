use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Represents a single crawled page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawledPage {
    /// URL of the page
    pub url: String,
    /// HTTP status code
    pub status: u16,
    /// Content type of the page
    pub content_type: String,
    /// Size of the page in bytes
    pub size: usize,
    /// Timestamp when the page was crawled
    pub timestamp: u64,
}

impl CrawledPage {
    /// Creates a new CrawledPage instance
    pub fn new(url: String, status: u16, content_type: String, size: usize) -> Self {
        // Get current timestamp
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        CrawledPage {
            url,
            status,
            content_type,
            size,
            timestamp,
        }
    }
}

/// Represents a complete crawl session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlReport {
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
    pub end_time: u64,
    /// Crawler identifier (could be a wallet address)
    pub crawler_id: String,
}

impl CrawlReport {
    /// Creates a new CrawlReport instance
    pub fn new(domain: String, crawler_id: String) -> Self {
        let start_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        CrawlReport {
            domain,
            pages_count: 0,
            total_size: 0,
            pages: Vec::new(),
            start_time,
            end_time: 0,
            crawler_id,
        }
    }

    /// Adds a page to the report
    pub fn add_page(&mut self, page: CrawledPage) {
        self.total_size += page.size;
        self.pages.push(page);
        self.pages_count = self.pages.len();
    }

    /// Finalizes the report by setting the end time
    pub fn finalize(&mut self) {
        self.end_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Calculates the duration of the crawl in seconds
    pub fn duration(&self) -> u64 {
        if self.end_time == 0 {
            // Crawl not yet completed
            return 0;
        }
        self.end_time - self.start_time
    }

    /// Calculates the crawl speed in pages per second
    pub fn pages_per_second(&self) -> f64 {
        let duration = self.duration();
        if duration == 0 {
            return 0.0;
        }
        self.pages_count as f64 / duration as f64
    }
} 