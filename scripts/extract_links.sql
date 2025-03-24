-- This script extracts links from HTML content and updates the is_javascript_dependent flag

-- First, update the is_javascript_dependent flag
UPDATE crawled_pages
SET 
    is_javascript_dependent = 
        CASE
            -- Check for common JavaScript framework signs
            WHEN html_content LIKE '%angular.%' OR 
                 html_content LIKE '%react.%' OR
                 html_content LIKE '%vue.%' OR
                 html_content LIKE '%ember.%' OR
                 html_content LIKE '%id="root"%' OR  -- Common React root
                 html_content LIKE '%app-root%' OR   -- Angular root
                 html_content LIKE '%data-reactroot%' OR 
                 html_content LIKE '%ng-app%' OR
                 html_content LIKE '%v-app%' THEN 1
            -- Check for common script patterns
            WHEN html_content LIKE '%webpack%' OR
                 html_content LIKE '%<noscript>Please enable JavaScript%' OR
                 html_content LIKE '%<noscript>This site requires JavaScript%' THEN 1
            ELSE 0
        END,
    javascript_dependency_reasons =
        CASE
            WHEN html_content LIKE '%angular.%' OR html_content LIKE '%ng-app%' THEN 'Angular framework detected'
            WHEN html_content LIKE '%react.%' OR html_content LIKE '%id="root"%' OR html_content LIKE '%data-reactroot%' THEN 'React framework detected'
            WHEN html_content LIKE '%vue.%' OR html_content LIKE '%v-app%' THEN 'Vue.js framework detected'
            WHEN html_content LIKE '%ember.%' THEN 'Ember.js framework detected'
            WHEN html_content LIKE '%webpack%' THEN 'Webpack bundled JavaScript detected'
            WHEN html_content LIKE '%<noscript>%' THEN 'Site requires JavaScript enabled'
            ELSE NULL
        END
WHERE 
    html_content IS NOT NULL;

-- Extract title from HTML
UPDATE crawled_pages
SET title = (
    SELECT 
        substr(
            substr(html_content, instr(html_content, '<title>') + 7),
            1,
            instr(substr(html_content, instr(html_content, '<title>') + 7), '</title>') - 1
        )
    FROM crawled_pages cp2
    WHERE cp2.id = crawled_pages.id
      AND html_content LIKE '%<title>%</title>%'
)
WHERE 
    html_content IS NOT NULL;

-- A simpler approach for link extraction using the LIKE operator
UPDATE crawled_pages
SET extracted_links = (
    SELECT json_group_array(link) FROM (
        WITH RECURSIVE split_html(remaining, processed) AS (
            -- Initial values
            SELECT html_content, '' FROM crawled_pages WHERE id = crawled_pages.id
            UNION ALL
            -- Recursively process the HTML
            SELECT
                substr(remaining, instr(remaining, 'href="') + 6), -- Skip to after 'href="'
                substr(remaining, instr(remaining, 'href="') + 6, instr(substr(remaining, instr(remaining, 'href="') + 6), '"'))
            FROM split_html
            WHERE instr(remaining, 'href="') > 0
        )
        -- Extract hrefs
        SELECT 
            TRIM(processed, '"') as link
        FROM split_html
        WHERE processed != '' AND length(processed) < 500 -- Avoid overly long "hrefs"
        LIMIT 1000 -- Reasonable limit
    )
)
WHERE html_content IS NOT NULL AND html_content LIKE '%href=%'; 