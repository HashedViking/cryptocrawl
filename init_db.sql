-- Database Initialization and Schema Management Script for Cryptocrawl

-- Enable foreign keys
PRAGMA foreign_keys = ON;

-- Part 1: Create or recreate basic database tables
-- ------------------------------------------------

-- Drop existing views first to avoid dependencies
DROP VIEW IF EXISTS v_crawled_pages;
DROP VIEW IF EXISTS v_js_dependency;

-- Drop existing tables if requested (commented out by default for safety)
-- DROP TABLE IF EXISTS crawled_pages;
-- DROP TABLE IF EXISTS crawl_reports;
-- DROP TABLE IF EXISTS crawl_results;
-- DROP TABLE IF EXISTS wallet_history;
-- DROP TABLE IF EXISTS tasks;

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

-- Part 2: Enhanced crawled_pages table for full content storage
-- ------------------------------------------------------------

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

-- Part 3: Create views for analysis
-- --------------------------------

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

-- Create view for JavaScript dependency analysis
CREATE VIEW IF NOT EXISTS v_js_dependency AS
SELECT 
    domain,
    COUNT(*) AS total_pages,
    SUM(CASE WHEN is_javascript_dependent = 1 THEN 1 ELSE 0 END) AS js_dependent_pages,
    ROUND(100.0 * SUM(CASE WHEN is_javascript_dependent = 1 THEN 1 ELSE 0 END) / COUNT(*), 2) AS js_dependency_percentage,
    GROUP_CONCAT(DISTINCT javascript_dependency_reasons) AS dependency_reasons
FROM 
    crawled_pages
GROUP BY 
    domain
ORDER BY 
    js_dependency_percentage DESC;

-- Part 4: Migration helper (uncomment and run as needed)
-- -----------------------------------------------------

-- Populate crawled_pages from existing crawl_results (if needed)
/*
INSERT OR IGNORE INTO crawled_pages (task_id, url, domain, status, size, html)
SELECT 
    cr.task_id,
    p.url,
    cr.domain,
    200, -- Assuming success status
    CASE WHEN p.html IS NULL THEN 0 ELSE length(p.html) END,
    p.html
FROM 
    crawl_results cr,
    json_each(cr.pages) pages,
    json_tree(pages.value) p
WHERE 
    p.key = 'url' OR p.key = 'html';
*/

-- Update JavaScript dependency detection
/*
UPDATE crawled_pages
SET 
    is_javascript_dependent = CASE
        WHEN html LIKE '%<noscript>%enable JavaScript%</noscript>%' THEN 1
        WHEN html LIKE '%<noscript>%browser%</noscript>%' THEN 1
        WHEN html LIKE '%ember.%js%' THEN 1
        WHEN html LIKE '%react.%js%' THEN 1
        WHEN html LIKE '%angular.%js%' THEN 1
        WHEN html LIKE '%vue.%js%' THEN 1
        ELSE 0
    END,
    javascript_dependency_reasons = CASE
        WHEN html LIKE '%<noscript>%enable JavaScript%</noscript>%' THEN 'noscript warning found'
        WHEN html LIKE '%<noscript>%browser%</noscript>%' THEN 'noscript warning found'
        WHEN html LIKE '%ember.%js%' THEN 'Ember.js application detected'
        WHEN html LIKE '%react.%js%' THEN 'React framework detected'
        WHEN html LIKE '%angular.%js%' THEN 'Angular framework detected'
        WHEN html LIKE '%vue.%js%' THEN 'Vue.js framework detected'
        ELSE ''
    END
WHERE 
    is_javascript_dependent = 0 OR javascript_dependency_reasons IS NULL;
*/

-- Extract links from HTML (recursive SQL to find all href attributes)
/*
UPDATE crawled_pages
SET extracted_links = (
    SELECT json_group_array(link) FROM (
        WITH RECURSIVE
        links(link, pos) AS (
            SELECT 
                trim(substr(html, instr(html, 'href="') + 6, instr(substr(html, instr(html, 'href="') + 6), '"')-1)),
                instr(html, 'href="') + 6 + instr(substr(html, instr(html, 'href="') + 6), '"')
            WHERE
                instr(html, 'href="') > 0
            UNION ALL
            SELECT 
                trim(substr(html, instr(substr(html, pos), 'href="') + pos + 6, 
                     instr(substr(html, instr(substr(html, pos), 'href="') + pos + 6), '"')-1)),
                pos + 6 + instr(substr(html, instr(substr(html, pos), 'href="') + pos + 6), '"')
            FROM 
                links
            WHERE 
                instr(substr(html, pos), 'href="') > 0
        )
        SELECT link FROM links 
        WHERE link NOT LIKE '#%' AND link NOT LIKE 'javascript:%' AND length(link) > 0
        LIMIT 1000
    )
)
WHERE 
    extracted_links IS NULL AND html IS NOT NULL;
*/

-- Analyze database for better performance
ANALYZE; 