-- Analysis of JavaScript dependency in crawled pages

-- Create view for JS dependency analysis if it doesn't exist
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

-- Display JavaScript dependency by domain
.mode column
.headers on
.width 30 10 10 10 40
SELECT 
    domain, 
    total_pages, 
    js_dependent_pages, 
    js_dependency_percentage || '%' AS dependency_rate,
    dependency_reasons
FROM 
    v_js_dependency;

-- Find pages with highest link counts (potential hub pages)
.echo
.echo "Top Hub Pages (Most outgoing links):"
.echo "-----------------------------------"
SELECT 
    url, 
    domain,
    (SELECT COUNT(*) FROM json_each(extracted_links)) AS link_count,
    title
FROM 
    crawled_pages
ORDER BY 
    link_count DESC
LIMIT 10;

-- Find JavaScript frameworks usage
.echo
.echo "JavaScript Framework Usage:"
.echo "-------------------------"
SELECT 
    'React' AS framework,
    COUNT(*) AS page_count,
    ROUND(100.0 * COUNT(*) / (SELECT COUNT(*) FROM crawled_pages), 2) || '%' AS usage_rate
FROM 
    crawled_pages
WHERE 
    html LIKE '%react.%js%' OR html LIKE '%/react/%'
UNION ALL
SELECT 
    'Angular' AS framework,
    COUNT(*) AS page_count,
    ROUND(100.0 * COUNT(*) / (SELECT COUNT(*) FROM crawled_pages), 2) || '%' AS usage_rate
FROM 
    crawled_pages
WHERE 
    html LIKE '%angular.%js%' OR html LIKE '%/angular/%'
UNION ALL
SELECT 
    'Vue' AS framework,
    COUNT(*) AS page_count,
    ROUND(100.0 * COUNT(*) / (SELECT COUNT(*) FROM crawled_pages), 2) || '%' AS usage_rate
FROM 
    crawled_pages
WHERE 
    html LIKE '%vue.%js%' OR html LIKE '%/vue/%'
UNION ALL
SELECT 
    'Svelte' AS framework,
    COUNT(*) AS page_count,
    ROUND(100.0 * COUNT(*) / (SELECT COUNT(*) FROM crawled_pages), 2) || '%' AS usage_rate
FROM 
    crawled_pages
WHERE 
    html LIKE '%svelte.%js%' OR html LIKE '%/svelte/%'
UNION ALL
SELECT 
    'Ember' AS framework,
    COUNT(*) AS page_count,
    ROUND(100.0 * COUNT(*) / (SELECT COUNT(*) FROM crawled_pages), 2) || '%' AS usage_rate
FROM 
    crawled_pages
WHERE 
    html LIKE '%ember.%js%' OR html LIKE '%/ember/%'
ORDER BY 
    page_count DESC;

-- Common JavaScript dependency patterns
.echo
.echo "Common JavaScript Dependency Patterns:"
.echo "------------------------------------"
SELECT 
    'NoScript warnings' AS pattern,
    COUNT(*) AS occurrences,
    ROUND(100.0 * COUNT(*) / (SELECT COUNT(*) FROM crawled_pages), 2) || '%' AS frequency
FROM 
    crawled_pages
WHERE 
    html LIKE '%<noscript>%enable JavaScript%</noscript>%'
UNION ALL
SELECT 
    'Explicit JS requirement' AS pattern,
    COUNT(*) AS occurrences,
    ROUND(100.0 * COUNT(*) / (SELECT COUNT(*) FROM crawled_pages), 2) || '%' AS frequency
FROM 
    crawled_pages
WHERE 
    html LIKE '%This site requires JavaScript%'
UNION ALL
SELECT 
    'JS-only navigation' AS pattern,
    COUNT(*) AS occurrences,
    ROUND(100.0 * COUNT(*) / (SELECT COUNT(*) FROM crawled_pages), 2) || '%' AS frequency
FROM 
    crawled_pages
WHERE 
    html LIKE '%onclick=%' AND html LIKE '%href="javascript:%'
ORDER BY 
    occurrences DESC;

-- Domain statistics
.echo
.echo "Domain Statistics:"
.echo "-----------------"
SELECT 
    domain,
    COUNT(*) AS pages,
    ROUND(AVG(size)/1024, 2) AS avg_size_kb,
    MAX(size)/1024 AS max_size_kb,
    MIN(size)/1024 AS min_size_kb,
    SUM(size)/1024/1024 AS total_size_mb,
    ROUND(AVG((SELECT COUNT(*) FROM json_each(extracted_links))), 2) AS avg_links
FROM 
    crawled_pages
GROUP BY 
    domain
ORDER BY 
    pages DESC;

-- Content analysis
.echo
.echo "Content Type Analysis:"
.echo "---------------------"
SELECT 
    COALESCE(content_type, 'Unknown') AS content_type,
    COUNT(*) AS page_count,
    ROUND(100.0 * COUNT(*) / (SELECT COUNT(*) FROM crawled_pages), 2) || '%' AS percentage
FROM 
    crawled_pages
GROUP BY 
    content_type
ORDER BY 
    page_count DESC;

-- Status code distribution
.echo
.echo "HTTP Status Code Distribution:"
.echo "----------------------------"
SELECT 
    status AS http_status,
    COUNT(*) AS page_count,
    ROUND(100.0 * COUNT(*) / (SELECT COUNT(*) FROM crawled_pages), 2) || '%' AS percentage
FROM 
    crawled_pages
GROUP BY 
    status
ORDER BY 
    page_count DESC; 