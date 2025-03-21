use crate::models::{Task, CrawlResult, CrawledPage, CrawlStatus};
use anyhow::{Result, Context, anyhow};
use rusqlite::{params, Connection, OptionalExtension};
use log::{info, warn, error};
use serde_json::Value;
use std::path::Path;
use uuid::Uuid;

/// Database for storing tasks and crawl results
pub struct Database {
    /// SQLite connection
    conn: Connection,
}

impl Database {
    /// Create a new database instance
    pub fn new(db_path: &Path) -> Result<Self> {
        // Connect to database
        let conn = Connection::open(db_path)
            .with_context(|| format!("Failed to open database at {:?}", db_path))?;
        
        // Create new database instance
        let mut db = Self { conn };
        
        // Initialize database
        db.init()?;
        
        Ok(db)
    }
    
    /// Initialize database tables
    fn init(&mut self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                target_url TEXT NOT NULL,
                max_depth INTEGER NOT NULL,
                follow_subdomains INTEGER NOT NULL,
                max_links INTEGER,
                created_at INTEGER NOT NULL,
                assigned_at INTEGER,
                incentive_amount INTEGER NOT NULL
            )",
            [],
        )?;
        
        self.conn.execute(
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
        
        self.conn.execute(
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
        
        Ok(())
    }
    
    /// Save a task to the database
    pub fn save_task(&mut self, task: &Task) -> Result<()> {
        self.conn.execute(
            "INSERT INTO tasks (
                id, target_url, max_depth, follow_subdomains, max_links, 
                created_at, assigned_at, incentive_amount
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                task.id,
                task.target_url,
                task.max_depth,
                task.follow_subdomains as i32,
                task.max_links,
                task.created_at,
                task.assigned_at,
                task.incentive_amount,
            ],
        )?;
        
        Ok(())
    }
    
    /// Get a task by ID
    pub fn get_task(&self, task_id: &str) -> Result<Option<Task>> {
        self.conn.query_row(
            "SELECT id, target_url, max_depth, follow_subdomains, max_links, 
                    created_at, assigned_at, incentive_amount
             FROM tasks WHERE id = ?",
            [task_id],
            |row| {
                Ok(Task {
                    id: row.get(0)?,
                    target_url: row.get(1)?,
                    max_depth: row.get(2)?,
                    follow_subdomains: row.get::<_, i32>(3)? != 0,
                    max_links: row.get(4)?,
                    created_at: row.get(5)?,
                    assigned_at: row.get(6)?,
                    incentive_amount: row.get(7)?,
                })
            },
        ).optional().with_context(|| format!("Failed to get task {}", task_id))
    }
    
    /// Get all tasks
    pub fn get_all_tasks(&self) -> Result<Vec<Task>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, target_url, max_depth, follow_subdomains, max_links, 
                    created_at, assigned_at, incentive_amount
             FROM tasks
             ORDER BY created_at DESC"
        )?;
        
        let tasks = stmt.query_map([], |row| {
            Ok(Task {
                id: row.get(0)?,
                target_url: row.get(1)?,
                max_depth: row.get(2)?,
                follow_subdomains: row.get::<_, i32>(3)? != 0,
                max_links: row.get(4)?,
                created_at: row.get(5)?,
                assigned_at: row.get(6)?,
                incentive_amount: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
        
        Ok(tasks)
    }
    
    /// Save a crawl result to the database
    pub fn save_crawl_result(&mut self, result: &CrawlResult) -> Result<()> {
        // Serialize pages to JSON
        let pages_json = serde_json::to_string(&result.pages)?;
        
        // Insert crawl result
        self.conn.execute(
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
    pub fn update_crawl_result(&mut self, result: &CrawlResult) -> Result<()> {
        // Serialize pages to JSON
        let pages_json = serde_json::to_string(&result.pages)?;
        
        // Update crawl result
        let rows_affected = self.conn.execute(
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
        self.conn.query_row(
            "SELECT task_id, domain, status, pages_count, pages, total_size,
                    start_time, end_time, transaction_hash, incentives_received
             FROM crawl_results WHERE task_id = ?",
            [task_id],
            |row| {
                // Parse status
                let status_str: String = row.get(2)?;
                let status = match status_str.as_str() {
                    "InProgress" => CrawlStatus::InProgress,
                    "Completed" => CrawlStatus::Completed,
                    "Failed" => CrawlStatus::Failed,
                    "Verified" => CrawlStatus::Verified,
                    "Rejected" => CrawlStatus::Rejected,
                    _ => return Err(rusqlite::Error::InvalidColumnType(2, "Invalid crawl status".to_string())),
                };
                
                // Parse pages
                let pages_json: String = row.get(4)?;
                let pages: Vec<CrawledPage> = serde_json::from_str(&pages_json)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e)))?;
                
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
            },
        ).optional().with_context(|| format!("Failed to get crawl result {}", task_id))
    }
    
    /// Get all crawl results
    pub fn get_all_crawl_results(&self) -> Result<Vec<CrawlResult>> {
        let mut stmt = self.conn.prepare(
            "SELECT task_id, domain, status, pages_count, pages, total_size,
                    start_time, end_time, transaction_hash, incentives_received
             FROM crawl_results
             ORDER BY start_time DESC"
        )?;
        
        let results = stmt.query_map([], |row| {
            // Parse status
            let status_str: String = row.get(2)?;
            let status = match status_str.as_str() {
                "InProgress" => CrawlStatus::InProgress,
                "Completed" => CrawlStatus::Completed,
                "Failed" => CrawlStatus::Failed,
                "Verified" => CrawlStatus::Verified,
                "Rejected" => CrawlStatus::Rejected,
                _ => return Err(rusqlite::Error::InvalidColumnType(2, "Invalid crawl status".to_string())),
            };
            
            // Parse pages
            let pages_json: String = row.get(4)?;
            let pages: Vec<CrawledPage> = serde_json::from_str(&pages_json)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e)))?;
            
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
        })?
        .collect::<Result<Vec<_>, _>>()?;
        
        Ok(results)
    }
    
    /// Add wallet history entry
    pub fn add_wallet_history(
        &mut self,
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
        
        self.conn.execute(
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
    pub fn get_wallet_history(&self, limit: Option<usize>) -> Result<Vec<(String, i64, u64, String, Option<String>)>> {
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
        
        let mut stmt = self.conn.prepare(&query)?;
        
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
} 