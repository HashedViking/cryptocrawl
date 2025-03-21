use crate::models::{Task, TaskStatus, CrawlReport, CrawledPage};
use anyhow::{anyhow, Result, Context};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::fs;
use serde_json;
use log::info;

/// Manages the database for the manager
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Create a new database connection from a path
    pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        // Get the path
        let path = db_path.as_ref();
        
        // Log the database location
        info!("Opening database at {:?}", path);
        
        // Check if the parent directory exists, if not create it
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                info!("Creating directory: {:?}", parent);
                fs::create_dir_all(parent)
                    .context(format!("Failed to create directory {:?}", parent))?;
            }
        }
        
        // Check if the database file exists
        let db_exists = path.exists();
        
        // Connect to the database
        let conn = Connection::open(path)
            .context(format!("Failed to open database at {:?}", path))?;
        
        // Create a new instance
        let mut db = Database { conn };
        
        // Initialize the database if it's new
        if !db_exists {
            db.init_database()?;
        }
        
        Ok(db)
    }
    
    /// Create a new database instance from a string path
    pub fn from_path(db_path: &str) -> Result<Self> {
        Self::new(PathBuf::from(db_path))
    }
    
    /// Initialize the database schema
    fn init_database(&mut self) -> Result<()> {
        info!("Initializing database tables");
        
        // Create tasks table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                target_url TEXT NOT NULL,
                max_depth INTEGER NOT NULL,
                follow_subdomains INTEGER NOT NULL,
                max_links INTEGER,
                created_at INTEGER NOT NULL,
                assigned_at INTEGER,
                completed_at INTEGER,
                status TEXT NOT NULL,
                assigned_to TEXT,
                incentive_amount INTEGER NOT NULL
            )",
            [],
        )?;
        
        // Create reports table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS reports (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id TEXT NOT NULL,
                client_id TEXT NOT NULL,
                domain TEXT NOT NULL,
                pages_count INTEGER NOT NULL,
                total_size INTEGER NOT NULL,
                pages TEXT NOT NULL,
                start_time INTEGER NOT NULL,
                end_time INTEGER NOT NULL,
                verified INTEGER NOT NULL,
                verification_score REAL,
                verification_notes TEXT,
                FOREIGN KEY (task_id) REFERENCES tasks(id)
            )",
            [],
        )?;
        
        info!("Database tables initialized successfully");
        Ok(())
    }
    
    /// Create a new task
    pub fn create_task(&self, task: &Task) -> Result<()> {
        self.conn.execute(
            "INSERT INTO tasks (
                id, target_url, max_depth, follow_subdomains, max_links,
                created_at, assigned_at, completed_at, status, assigned_to, incentive_amount
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                task.id,
                task.target_url,
                task.max_depth,
                if task.follow_subdomains { 1 } else { 0 },
                task.max_links,
                task.created_at,
                task.assigned_at,
                task.completed_at,
                format!("{:?}", task.status),
                task.assigned_to,
                task.incentive_amount,
            ],
        )?;
        
        Ok(())
    }
    
    /// Get a task by ID
    pub fn get_task(&self, task_id: &str) -> Result<Option<Task>> {
        let mut stmt = self.conn.prepare(
            "SELECT 
                id, target_url, max_depth, follow_subdomains, max_links,
                created_at, assigned_at, completed_at, status, assigned_to, incentive_amount
            FROM tasks
            WHERE id = ?"
        )?;
        
        let task_result = stmt.query_row(params![task_id], |row| {
            let status_str: String = row.get(8)?;
            let status = match status_str.as_str() {
                "Pending" => TaskStatus::Pending,
                "Assigned" => TaskStatus::Assigned,
                "InProgress" => TaskStatus::InProgress,
                "Completed" => TaskStatus::Completed,
                "Failed" => TaskStatus::Failed,
                "Verified" => TaskStatus::Verified,
                "Rejected" => TaskStatus::Rejected,
                _ => TaskStatus::Pending,
            };
            
            Ok(Task {
                id: row.get(0)?,
                target_url: row.get(1)?,
                max_depth: row.get(2)?,
                follow_subdomains: row.get::<_, i32>(3)? != 0,
                max_links: row.get(4)?,
                created_at: row.get(5)?,
                assigned_at: row.get(6)?,
                completed_at: row.get(7)?,
                status,
                assigned_to: row.get(9)?,
                incentive_amount: row.get(10)?,
            })
        });
        
        match task_result {
            Ok(task) => Ok(Some(task)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(anyhow!(e)),
        }
    }
    
    /// Update a task
    pub fn update_task(&self, task: &Task) -> Result<()> {
        self.conn.execute(
            "UPDATE tasks SET
                target_url = ?,
                max_depth = ?,
                follow_subdomains = ?,
                max_links = ?,
                assigned_at = ?,
                completed_at = ?,
                status = ?,
                assigned_to = ?,
                incentive_amount = ?
            WHERE id = ?",
            params![
                task.target_url,
                task.max_depth,
                if task.follow_subdomains { 1 } else { 0 },
                task.max_links,
                task.assigned_at,
                task.completed_at,
                format!("{:?}", task.status),
                task.assigned_to,
                task.incentive_amount,
                task.id,
            ],
        )?;
        
        Ok(())
    }
    
    /// Get all pending tasks
    pub fn get_pending_tasks(&self) -> Result<Vec<Task>> {
        let mut stmt = self.conn.prepare(
            "SELECT 
                id, target_url, max_depth, follow_subdomains, max_links,
                created_at, assigned_at, completed_at, status, assigned_to, incentive_amount
            FROM tasks
            WHERE status = 'Pending'"
        )?;
        
        let task_iter = stmt.query_map([], |row| {
            let status_str: String = row.get(8)?;
            let status = match status_str.as_str() {
                "Pending" => TaskStatus::Pending,
                "Assigned" => TaskStatus::Assigned,
                "InProgress" => TaskStatus::InProgress,
                "Completed" => TaskStatus::Completed,
                "Failed" => TaskStatus::Failed,
                "Verified" => TaskStatus::Verified,
                "Rejected" => TaskStatus::Rejected,
                _ => TaskStatus::Pending,
            };
            
            Ok(Task {
                id: row.get(0)?,
                target_url: row.get(1)?,
                max_depth: row.get(2)?,
                follow_subdomains: row.get::<_, i32>(3)? != 0,
                max_links: row.get(4)?,
                created_at: row.get(5)?,
                assigned_at: row.get(6)?,
                completed_at: row.get(7)?,
                status,
                assigned_to: row.get(9)?,
                incentive_amount: row.get(10)?,
            })
        })?;
        
        let mut tasks = Vec::new();
        for task in task_iter {
            tasks.push(task?);
        }
        
        Ok(tasks)
    }
    
    /// Save a crawl report
    pub fn save_report(&self, report: &CrawlReport) -> Result<i64> {
        // Serialize pages to JSON
        let pages_json = serde_json::to_string(&report.pages)?;
        
        self.conn.execute(
            "INSERT INTO reports (
                task_id, client_id, domain, pages_count, total_size,
                pages, start_time, end_time, verified, verification_score, verification_notes
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                report.task_id,
                report.client_id,
                report.domain,
                report.pages_count,
                report.total_size,
                pages_json,
                report.start_time,
                report.end_time,
                if report.verified { 1 } else { 0 },
                report.verification_score,
                report.verification_notes,
            ],
        )?;
        
        Ok(self.conn.last_insert_rowid())
    }
    
    /// Get a report by task ID
    pub fn get_report_by_task(&self, task_id: &str) -> Result<Option<CrawlReport>> {
        let mut stmt = self.conn.prepare(
            "SELECT task_id, client_id, domain, pages_count, total_size, pages, 
             start_time, end_time, verified, verification_score, verification_notes 
             FROM reports WHERE task_id = ?"
        )?;
        
        let report_result = stmt.query_row(params![task_id], |row| {
            let task_id: String = row.get(0)?;
            let client_id: String = row.get(1)?;
            let domain: String = row.get(2)?;
            let pages_count: usize = row.get(3)?;
            let total_size: usize = row.get(4)?;
            let pages_json: String = row.get(5)?;
            let start_time: u64 = row.get(6)?;
            let end_time: Option<u64> = row.get(7)?;
            let verified: bool = row.get(8)?;
            let verification_score: Option<f64> = row.get(9)?;
            let verification_notes: Option<String> = row.get(10)?;
            
            Ok((
                task_id, client_id, domain, pages_count, total_size, pages_json,
                start_time, end_time, verified, verification_score, verification_notes
            ))
        });
        
        match report_result {
            Ok((task_id, client_id, domain, pages_count, total_size, pages_json,
                start_time, end_time, verified, verification_score, verification_notes)) => {
                
                // Parse pages JSON outside the query_row closure
                let pages: Vec<CrawledPage> = serde_json::from_str(&pages_json)
                    .context("Failed to parse pages JSON")?;
                
                Ok(Some(CrawlReport {
                    task_id,
                    client_id,
                    domain,
                    pages_count,
                    total_size,
                    pages,
                    start_time,
                    end_time,
                    verified,
                    verification_score,
                    verification_notes,
                }))
            },
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(anyhow!("Database error: {}", e)),
        }
    }
    
    /// Update a report's verification status
    pub fn update_report_verification(&self, task_id: &str, verified: bool, score: Option<f64>, notes: Option<String>) -> Result<()> {
        self.conn.execute(
            "UPDATE reports SET
                verified = ?,
                verification_score = ?,
                verification_notes = ?
            WHERE task_id = ?",
            params![
                if verified { 1 } else { 0 },
                score,
                notes,
                task_id,
            ],
        )?;
        
        Ok(())
    }
} 