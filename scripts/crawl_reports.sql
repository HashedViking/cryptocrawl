-- 1. Basic Crawl Summary
SELECT 
    cr.task_id,
    cr.domain,
    cr.status,
    cr.pages_count,
    cr.total_size,
    datetime(cr.start_time, 'unixepoch') as start_time,
    CASE WHEN cr.end_time IS NOT NULL THEN datetime(cr.end_time, 'unixepoch') ELSE NULL END as end_time,
    CASE WHEN cr.end_time IS NOT NULL THEN (cr.end_time - cr.start_time) ELSE NULL END as duration_seconds
FROM 
    crawl_results cr
ORDER BY 
    cr.start_time DESC;

-- 2. JavaScript Dependency Analysis
SELECT 
    cr.domain,
    COUNT(*) as total_pages,
    SUM(CASE WHEN cp.is_javascript_dependent = 1 THEN 1 ELSE 0 END) as js_dependent_pages,
    ROUND(SUM(CASE WHEN cp.is_javascript_dependent = 1 THEN 1 ELSE 0 END) * 100.0 / COUNT(*), 2) as js_dependent_percent,
    GROUP_CONCAT(DISTINCT cp.javascript_dependency_reasons, ', ') as dependency_reasons
FROM 
    crawl_results cr
JOIN 
    crawled_pages cp ON cr.task_id = cp.task_id
GROUP BY 
    cr.domain
ORDER BY 
    js_dependent_percent DESC;

-- 3. Most Common Content Types
SELECT 
    content_type,
    COUNT(*) as page_count,
    ROUND(AVG(size), 2) as avg_size,
    SUM(size) as total_size
FROM 
    crawled_pages
WHERE 
    content_type IS NOT NULL
GROUP BY 
    content_type
ORDER BY 
    page_count DESC;

-- 4. Top Linked Pages (Pages with most incoming links)
WITH all_links AS (
    SELECT 
        json_each.value as link
    FROM 
        crawled_pages,
        json_each(extracted_links)
    WHERE 
        extracted_links IS NOT NULL
)
SELECT 
    link,
    COUNT(*) as incoming_links
FROM 
    all_links
GROUP BY 
    link
HAVING 
    COUNT(*) > 1
ORDER BY 
    incoming_links DESC
LIMIT 20;

-- 5. Domain Crawl Efficiency
SELECT 
    cr.domain,
    cr.pages_count,
    CASE WHEN cr.end_time IS NOT NULL THEN (cr.end_time - cr.start_time) ELSE NULL END as duration_seconds,
    CASE WHEN cr.end_time IS NOT NULL THEN ROUND(cr.pages_count * 1.0 / (cr.end_time - cr.start_time), 2) ELSE NULL END as pages_per_second,
    ROUND(cr.total_size * 1.0 / (1024 * 1024), 2) as total_size_mb,
    CASE WHEN cr.end_time IS NOT NULL THEN ROUND(cr.total_size * 1.0 / (1024 * 1024) / (cr.end_time - cr.start_time), 2) ELSE NULL END as mb_per_second
FROM 
    crawl_results cr
WHERE 
    cr.end_time IS NOT NULL
ORDER BY 
    pages_per_second DESC; 