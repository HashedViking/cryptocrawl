-- Add a crawled_pages table to store detailed page information
CREATE TABLE IF NOT EXISTS crawled_pages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT NOT NULL,
    url TEXT NOT NULL,
    title TEXT,
    size INTEGER NOT NULL,
    timestamp INTEGER NOT NULL,
    content_type TEXT,
    status_code INTEGER,
    html_content TEXT,  -- Store the actual HTML content
    extracted_links TEXT, -- Store all extracted links as JSON array
    is_javascript_dependent INTEGER DEFAULT 0,
    javascript_dependency_reasons TEXT,
    FOREIGN KEY (task_id) REFERENCES tasks(id)
);

-- Create indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_crawled_pages_task_id ON crawled_pages(task_id);
CREATE INDEX IF NOT EXISTS idx_crawled_pages_url ON crawled_pages(url);

-- Create a view for easy querying of pages with their domain
CREATE VIEW IF NOT EXISTS v_crawled_pages AS
SELECT 
    cp.*,
    cr.domain,
    cr.status as crawl_status
FROM 
    crawled_pages cp
JOIN 
    crawl_results cr ON cp.task_id = cr.task_id; 