-- Drop existing tables and views if they exist
DROP VIEW IF EXISTS v_crawled_pages;
DROP TABLE IF EXISTS crawled_pages;
DROP TABLE IF EXISTS crawl_reports;
DROP TABLE IF EXISTS crawl_results;
DROP TABLE IF EXISTS wallet_history;
DROP TABLE IF EXISTS tasks;

-- Create tasks table
CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    url TEXT NOT NULL,
    max_depth INTEGER NOT NULL,
    follow_subdomains INTEGER NOT NULL,
    max_links INTEGER,
    created_at INTEGER NOT NULL,
    assigned_at INTEGER,
    incentive_amount INTEGER NOT NULL
);

-- Create crawl_results table
CREATE TABLE IF NOT EXISTS crawl_results (
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
);

-- Create wallet_history table
CREATE TABLE IF NOT EXISTS wallet_history (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    amount INTEGER NOT NULL,
    timestamp INTEGER NOT NULL,
    transaction_hash TEXT NOT NULL,
    description TEXT,
    FOREIGN KEY (task_id) REFERENCES tasks(id)
);

-- Create crawl_reports table
CREATE TABLE IF NOT EXISTS crawl_reports (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT NOT NULL,
    pages_crawled INTEGER NOT NULL,
    total_size_bytes INTEGER NOT NULL,
    crawl_duration_ms INTEGER NOT NULL,
    transaction_signature TEXT,
    timestamp INTEGER NOT NULL,
    FOREIGN KEY (task_id) REFERENCES tasks (id)
);

-- Create crawled_pages table for storing individual pages with full content
CREATE TABLE IF NOT EXISTS crawled_pages (
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
);

-- Create indexes for crawled_pages
CREATE INDEX IF NOT EXISTS idx_crawled_pages_task_id ON crawled_pages(task_id);
CREATE INDEX IF NOT EXISTS idx_crawled_pages_domain ON crawled_pages(domain);

-- Create view for easy querying of crawled pages
CREATE VIEW IF NOT EXISTS v_crawled_pages AS
SELECT 
    cp.*,
    cr.status as crawl_status,
    (SELECT COUNT(*) FROM json_each(extracted_links)) AS link_count
FROM 
    crawled_pages cp
LEFT JOIN 
    crawl_results cr ON cp.task_id = cr.task_id; 