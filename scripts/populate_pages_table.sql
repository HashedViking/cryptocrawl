-- Insert data from the crawl_results table into the crawled_pages table
INSERT INTO crawled_pages (
    task_id,
    url,
    size,
    timestamp,
    content_type,
    status_code,
    html_content
)
SELECT
    cr.task_id,
    json_extract(value, '$.url'),
    json_extract(value, '$.size'),
    json_extract(value, '$.timestamp'),
    json_extract(value, '$.content_type'),
    json_extract(value, '$.status_code'),
    json_extract(value, '$.body')
FROM
    crawl_results cr,
    json_each(cr.pages)
WHERE
    json_extract(value, '$.url') IS NOT NULL
    AND NOT EXISTS (
        SELECT 1 FROM crawled_pages cp
        WHERE cp.task_id = cr.task_id
        AND cp.url = json_extract(value, '$.url')
    ); 