use crate::models::{Task, CrawlResult, CrawledPage, CrawlStatus, CrawlReport};
use anyhow::{Result, Context};
use rusqlite::{params, Connection};
use log::{info, warn};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use regex;
use std::sync::Arc;
use std::sync::Mutex;

/// Type alias for a wallet history entry
pub type WalletHistoryEntry = (String, i64, u64, String, Option<String>);

/// Database connection wrapper
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
    path: PathBuf,
}

impl Database {
    /// Create a new database connection
    pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        // Get the path
        let path = db_path.as_ref().to_path_buf();
        
        // Log the database location
        info!("Opening database at {:?}", path);
        
        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .context(format!("Failed to create directory: {:?}", parent))?;
            }
        }
        
        // Connect to database
        let conn = Connection::open(&path)
            .with_context(|| format!("Failed to open database at {:?}", path))?;
        
        // Create new database instance
        let db = Self { conn: Arc::new(Mutex::new(conn)), path };
        
        Ok(db)
    }
    
    /// Create a new database instance from a string path
    pub fn from_path(db_path: &str) -> Result<Self> {
        Self::new(PathBuf::from(db_path))
    }
    
    /// Initialize database tables
    pub fn init_tables(&self) -> Result<()> {
        info!("Initializing database tables");
        
        let conn = self.conn.lock().unwrap();
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                max_depth INTEGER NOT NULL,
                follow_subdomains INTEGER NOT NULL,
                max_links INTEGER,
                created_at INTEGER NOT NULL,
                assigned_at INTEGER,
                incentive_amount INTEGER NOT NULL
            )",
            [],
        )?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS crawl_results (
                task_id TEXT PRIMARY KEY,
                domain TEXT NOT NULL,
                status TEXT NOT NULL,
                pages_count INTEGER NOT NULL,
                pages TEXT NOT NULL,
                total_size INTEGER NOT NULL,
                start_time INTEGER NOT NULL,
                end_time INTEGER,
                transaction_hash TEXT,
                incentives_received INTEGER,
                FOREIGN KEY (task_id) REFERENCES tasks(id)
            )",
            [],
        )?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS wallet_history (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                amount INTEGER NOT NULL,
                timestamp INTEGER NOT NULL,
                transaction_hash TEXT NOT NULL,
                description TEXT,
                FOREIGN KEY (task_id) REFERENCES tasks(id)
            )",
            [],
        )?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS crawl_reports (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id TEXT NOT NULL,
                pages_crawled INTEGER NOT NULL,
                total_size_bytes INTEGER NOT NULL,
                crawl_duration_ms INTEGER NOT NULL,
                transaction_signature TEXT,
                timestamp INTEGER NOT NULL,
                FOREIGN KEY (task_id) REFERENCES tasks (id)
            )",
            [],
        )?;
        
        // Create crawled_pages table for storing individual pages with full content
        conn.execute(
            "CREATE TABLE IF NOT EXISTS crawled_pages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id TEXT NOT NULL,
                url TEXT NOT NULL,
                domain TEXT NOT NULL,
                status INTEGER,
                content_type TEXT,
                title TEXT,
                size INTEGER NOT NULL,
                html TEXT,
                fetched_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                is_javascript_dependent INTEGER DEFAULT 0,
                javascript_dependency_reasons TEXT,
                extracted_links TEXT,
                FOREIGN KEY (task_id) REFERENCES tasks(id),
                UNIQUE(url)
            )",
            [],
        )?;
        
        // Create indexes for crawled_pages
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_crawled_pages_task_id ON crawled_pages(task_id)",
            [],
        )?;
        
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_crawled_pages_domain ON crawled_pages(domain)",
            [],
        )?;
        
        // Create view for easy querying of crawled pages
        conn.execute(
            "CREATE VIEW IF NOT EXISTS v_crawled_pages AS
             SELECT 
                cp.*,
                cr.status as crawl_status,
                (SELECT COUNT(*) FROM json_each(extracted_links)) AS link_count
             FROM 
                crawled_pages cp
             LEFT JOIN 
                crawl_results cr ON cp.task_id = cr.task_id",
            [],
        )?;
        
        info!("Database tables initialized successfully");
        Ok(())
    }
    
    /// Initialize database (legacy method)
    pub fn init(&self) -> Result<()> {
        self.init_tables()
    }
    
    /// Save a task to the database
    pub fn save_task(&self, task: &Task) -> Result<()> {
        info!("Saving task {} to database", task.id);
        
        let conn = self.conn.lock().unwrap();
        
        // Convert boolean to integer
        let follow_subdomains_int: i32 = if task.follow_subdomains { 1 } else { 0 };
        
        // Insert task into database
        conn.execute(
            "INSERT OR REPLACE INTO tasks (
                id, url, max_depth, follow_subdomains, max_links,
                created_at, assigned_at, incentive_amount
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                task.id,
                task.target_url,
                task.max_depth,
                follow_subdomains_int,
                match task.max_links {
                    Some(links) => links as i64,
                    None => -1, // Use -1 to represent unlimited
                },
                task.created_at,
                task.assigned_at,
                task.incentive_amount,
            ],
        ).with_context(|| format!("Failed to save task with ID: {}", task.id))?;
        
        Ok(())
    }
    
    /// Get a task by ID
    pub fn get_task(&self, task_id: &str) -> Result<Option<Task>> {
        let conn = self.conn.lock().unwrap();
        
        let mut stmt = conn.prepare(
            "SELECT id, target_url, max_depth, follow_subdomains, max_links, 
                    created_at, assigned_at, incentive_amount
             FROM tasks WHERE id = ?"
        )?;
        
        let mut rows = stmt.query(params![task_id])?;
        
        if let Some(row) = rows.next()? {
            let max_links_val: i64 = row.get(4)?;
            let max_links = if max_links_val < 0 {
                None
            } else {
                Some(max_links_val as usize)
            };
            
            Ok(Some(Task {
                id: row.get(0)?,
                target_url: row.get(1)?,
                max_depth: row.get(2)?,
                follow_subdomains: row.get::<_, i32>(3)? != 0,
                max_links,
                created_at: row.get(5)?,
                assigned_at: row.get(6)?,
                incentive_amount: row.get(7)?,
            }))
        } else {
            Ok(None)
        }
    }
    
    /// Get all tasks
    pub fn get_all_tasks(&self) -> Result<Vec<Task>> {
        let conn = self.conn.lock().unwrap();
        
        let mut stmt = conn.prepare(
            "SELECT id, target_url, max_depth, follow_subdomains, max_links, 
                    created_at, assigned_at, incentive_amount
             FROM tasks
             ORDER BY created_at DESC"
        )?;
        
        let task_iter = stmt.query_map([], |row| {
            let max_links_val: i64 = row.get(4)?;
            let max_links = if max_links_val < 0 {
                None
            } else {
                Some(max_links_val as usize)
            };
            
            Ok(Task {
                id: row.get(0)?,
                target_url: row.get(1)?,
                max_depth: row.get(2)?,
                follow_subdomains: row.get::<_, i32>(3)? != 0,
                max_links,
                created_at: row.get(5)?,
                assigned_at: row.get(6)?,
                incentive_amount: row.get(7)?,
            })
        })?;
        
        let mut tasks = Vec::new();
        for task in task_iter {
            tasks.push(task?);
        }
        
        Ok(tasks)
    }
    
    /// Save a crawl result to the database
    pub fn save_crawl_result(&self, result: &CrawlResult) -> Result<()> {
        // Serialize pages to JSON
        let pages_json = serde_json::to_string(&result.pages)?;
        
        let conn = self.conn.lock().unwrap();
        
        // Insert crawl result
        conn.execute(
            "INSERT INTO crawl_results (
                task_id, domain, status, pages_count, pages, total_size,
                start_time, end_time, transaction_hash, incentives_received
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                result.task_id,
                result.domain,
                result.status.to_string(),
                result.pages_count,
                pages_json,
                result.total_size,
                result.start_time,
                result.end_time,
                result.transaction_hash,
                result.incentives_received,
            ],
        )?;
        
        Ok(())
    }
    
    /// Update an existing crawl result
    pub fn update_crawl_result(&self, result: &CrawlResult) -> Result<()> {
        // Serialize pages to JSON
        let pages_json = serde_json::to_string(&result.pages)?;
        
        let conn = self.conn.lock().unwrap();
        
        // Update crawl result
        let rows_affected = conn.execute(
            "UPDATE crawl_results SET 
                domain = ?, status = ?, pages_count = ?, pages = ?, 
                total_size = ?, start_time = ?, end_time = ?,
                transaction_hash = ?, incentives_received = ?
             WHERE task_id = ?",
            params![
                result.domain,
                result.status.to_string(),
                result.pages_count,
                pages_json,
                result.total_size,
                result.start_time,
                result.end_time,
                result.transaction_hash,
                result.incentives_received,
                result.task_id,
            ],
        )?;
        
        if rows_affected == 0 {
            warn!("No rows affected when updating crawl result {}", result.task_id);
        }
        
        Ok(())
    }
    
    /// Get a crawl result by task ID
    pub fn get_crawl_result(&self, task_id: &str) -> Result<Option<CrawlResult>> {
        let conn = self.conn.lock().unwrap();
        
        let mut stmt = conn.prepare(
            "SELECT task_id, domain, status, pages_count, pages, total_size,
                    start_time, end_time, transaction_hash, incentives_received
             FROM crawl_results WHERE task_id = ?"
        )?;
        
        let mut rows = stmt.query(params![task_id])?;
        
        if let Some(row) = rows.next()? {
            // Parse status
            let status_str: String = row.get(2)?;
            let status = match status_str.as_str() {
                "InProgress" => CrawlStatus::InProgress,
                "Completed" => CrawlStatus::Completed,
                "Failed" => CrawlStatus::Failed,
                "Verified" => CrawlStatus::Verified,
                "Rejected" => CrawlStatus::Rejected,
                _ => return Err(anyhow::anyhow!("Invalid crawl status: {}", status_str)),
            };
            
            // Parse pages
            let pages_json: String = row.get(4)?;
            let pages: Vec<CrawledPage> = serde_json::from_str(&pages_json)
                .with_context(|| format!("Failed to parse pages JSON for task {}", task_id))?;
            
            Ok(Some(CrawlResult {
                task_id: row.get(0)?,
                domain: row.get(1)?,
                status,
                pages_count: row.get(3)?,
                pages,
                total_size: row.get(5)?,
                start_time: row.get(6)?,
                end_time: row.get(7)?,
                transaction_hash: row.get(8)?,
                incentives_received: row.get(9)?,
            }))
        } else {
            Ok(None)
        }
    }
    
    /// Get all crawl results
    pub fn get_all_crawl_results(&self) -> Result<Vec<CrawlResult>> {
        let conn = self.conn.lock().unwrap();
        
        let mut stmt = conn.prepare(
            "SELECT task_id, domain, status, pages_count, pages, total_size,
                    start_time, end_time, transaction_hash, incentives_received
             FROM crawl_results
             ORDER BY start_time DESC"
        )?;
        
        let result_iter = stmt.query_map([], |row| {
            // Parse status
            let status_str: String = row.get(2)?;
            let status = match status_str.as_str() {
                "InProgress" => CrawlStatus::InProgress,
                "Completed" => CrawlStatus::Completed,
                "Failed" => CrawlStatus::Failed,
                "Verified" => CrawlStatus::Verified,
                "Rejected" => CrawlStatus::Rejected,
                _ => CrawlStatus::Failed, // Default to failed for unknown status
            };
            
            // Parse pages
            let pages_json: String = row.get(4)?;
            let pages: Vec<CrawledPage> = match serde_json::from_str(&pages_json) {
                Ok(p) => p,
                Err(_) => Vec::new(), // Empty vector on error
            };
            
            Ok(CrawlResult {
                task_id: row.get(0)?,
                domain: row.get(1)?,
                status,
                pages_count: row.get(3)?,
                pages,
                total_size: row.get(5)?,
                start_time: row.get(6)?,
                end_time: row.get(7)?,
                transaction_hash: row.get(8)?,
                incentives_received: row.get(9)?,
            })
        })?;
        
        let mut results = Vec::new();
        for result in result_iter {
            results.push(result?);
        }
        
        Ok(results)
    }
    
    /// Add wallet history entry
    pub fn add_wallet_history(
        &self,
        task_id: &str,
        amount: i64,
        transaction_hash: &str,
        description: Option<&str>,
    ) -> Result<()> {
        let id = Uuid::new_v4().to_string();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        let conn = self.conn.lock().unwrap();
        
        conn.execute(
            "INSERT INTO wallet_history (
                id, task_id, amount, timestamp, transaction_hash, description
            ) VALUES (?, ?, ?, ?, ?, ?)",
            params![
                id,
                task_id,
                amount,
                timestamp,
                transaction_hash,
                description,
            ],
        )?;
        
        Ok(())
    }
    
    /// Get wallet history entries
    pub fn get_wallet_history(&self, limit: Option<usize>) -> Result<Vec<WalletHistoryEntry>> {
        let limit_clause = match limit {
            Some(n) => format!("LIMIT {}", n),
            None => String::new(),
        };
        
        let query = format!(
            "SELECT task_id, amount, timestamp, transaction_hash, description
             FROM wallet_history
             ORDER BY timestamp DESC
             {}",
            limit_clause
        );
        
        let conn = self.conn.lock().unwrap();
        
        let mut stmt = conn.prepare(&query)?;
        
        let history = stmt.query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
        
        Ok(history)
    }
    
    /// Save a crawl report to the database
    pub fn save_crawl_report(&self, report: &CrawlReport) -> Result<()> {
        // Get current timestamp
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        // Use a separate connection for the transaction
        let conn = Connection::open(&self.path)
            .context("Failed to open separate connection for transaction")?;
        
        // Start a transaction
        conn.execute("BEGIN TRANSACTION", [])?;
        
        // Save the crawl report
        conn.execute(
            "INSERT INTO crawl_reports (
                task_id, pages_crawled, total_size_bytes, 
                crawl_duration_ms, transaction_signature, timestamp
            ) VALUES (?, ?, ?, ?, ?, ?)",
            params![
                report.task_id,
                report.pages_crawled as i64,
                report.total_size_bytes as i64,
                report.crawl_duration_ms as i64,
                report.transaction_signature,
                timestamp,
            ],
        ).context("Failed to save crawl report")?;
        
        // Save the crawled pages
        for page in &report.pages {
            conn.execute(
                "INSERT OR IGNORE INTO crawled_pages (
                    task_id, url, size, timestamp, content_type, status_code
                ) VALUES (?, ?, ?, ?, ?, ?)",
                params![
                    report.task_id,
                    page.url,
                    page.size as i64,
                    page.timestamp,
                    page.content_type,
                    page.status_code,
                ],
            ).context("Failed to save crawled page")?;
        }
        
        // Commit the transaction
        conn.execute("COMMIT", [])?;
        
        info!("Saved crawl report for task {} with {} pages", 
              report.task_id, report.pages_crawled);
        
        Ok(())
    }
    
    /// Extract title from HTML if available
    fn extract_title_from_html(&self, html: &str) -> Option<String> {
        let regex = regex::Regex::new(r"<title[^>]*>(.*?)</title>").ok()?;
        if let Some(title_match) = regex.captures(html) {
            title_match.get(1).map(|m| m.as_str().to_string())
        } else {
            None
        }
    }

    /// Save a crawled page to the database with full HTML content
    pub fn save_crawled_page(
        &self,
        task_id: &str,
        url: &str,
        domain: &str,
        status: i32,
        content_type: Option<&str>,
        size: i64,
        html: Option<&str>,
        is_javascript_dependent: bool,
        javascript_dependency_reasons: Option<String>,
    ) -> Result<()> {
        // Convert boolean to integer
        let js_dependent_int: i32 = if is_javascript_dependent { 1 } else { 0 };
        
        // Extract title from HTML if available
        let title = match html {
            Some(content) => self.extract_title_from_html(content),
            None => None,
        };

        let conn = self.conn.lock().unwrap();
        
        // Insert the page using UPSERT logic (INSERT OR REPLACE)
        conn.execute(
            "INSERT OR REPLACE INTO crawled_pages (
                task_id, url, domain, status, content_type, title, size, html,
                fetched_at, is_javascript_dependent, javascript_dependency_reasons
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), ?, ?)",
            params![
                task_id,
                url,
                domain,
                status,
                content_type,
                title,
                size,
                html,
                js_dependent_int,
                javascript_dependency_reasons,
            ],
        ).context("Failed to save crawled page")?;
        
        info!("Saved page to database: {}", url);
        Ok(())
    }
    
    /// Update extracted links for a crawled page
    pub fn update_crawled_page_links(&self, url: &str, links: &[String]) -> Result<()> {
        // Convert links to JSON
        let links_json = serde_json::to_string(links)
            .context("Failed to convert links to JSON")?;
        
        let conn = self.conn.lock().unwrap();
        
        // Update the page with the extracted links
        let rows_updated = conn.execute(
            "UPDATE crawled_pages SET extracted_links = ? WHERE url = ?",
            params![links_json, url],
        ).context("Failed to update page links")?;
        
        if rows_updated > 0 {
            info!("Updated {} links for page: {}", links.len(), url);
        } else {
            warn!("No page found to update links for URL: {}", url);
        }
        
        Ok(())
    }

    /// Add a crawled page to the database
    pub fn add_crawled_page(&self, task_id: &str, url: &str, domain: &str, status: i32, 
                            content_type: Option<&str>, title: Option<&str>, 
                            size: usize, html: Option<&str>) -> Result<i64> {
        info!("Adding crawled page to database: {}", url);
        
        let conn = self.conn.lock().unwrap();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
            
        let result = conn.execute(
            "INSERT INTO crawled_pages (task_id, url, domain, status, content_type, title, size, html, fetched_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                task_id,
                url,
                domain,
                status,
                content_type,
                title,
                size as i64,
                html,
                timestamp as i64
            ],
        )?;
        
        // Get the row ID of the inserted page
        let id = conn.last_insert_rowid();
        
        if result > 0 {
            info!("Added crawled page: {} (ID: {})", url, id);
        } else {
            warn!("Failed to add crawled page: {}", url);
        }
        
        Ok(id)
    }
    
    /// Update JavaScript dependency information for a URL
    pub fn update_js_dependency(&self, url: &str, is_dependent: bool, reasons: &str) -> Result<()> {
        info!("Updating JavaScript dependency info for URL: {}", url);
        
        let conn = self.conn.lock().unwrap();
        
        conn.execute(
            "UPDATE crawled_pages 
             SET is_javascript_dependent = ?, 
                 javascript_dependency_reasons = ? 
             WHERE url = ?",
            params![
                if is_dependent { 1 } else { 0 },
                reasons,
                url,
            ],
        ).with_context(|| format!("Failed to update JavaScript dependency for URL: {}", url))?;
        
        info!("JavaScript dependency info updated for {}", url);
        Ok(())
    }
    
    /// Check if a URL is already in the crawled_pages table
    pub fn is_url_crawled(&self, url: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM crawled_pages WHERE url = ?",
            params![url],
            |row| row.get(0),
        )?;
        
        Ok(count > 0)
    }
} 