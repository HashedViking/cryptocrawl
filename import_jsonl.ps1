# Import JSONL crawl data into database
param (
    [Parameter(Mandatory=$true)]
    [string]$JsonlFile,
    
    [Parameter(Mandatory=$false)]
    [string]$DatabaseFile = "data/crawler.db",
    
    [Parameter(Mandatory=$false)]
    [switch]$SkipBackup,
    
    [Parameter(Mandatory=$false)]
    [string]$TaskId = ""
)

# Verify files exist
if (-not (Test-Path $JsonlFile)) {
    Write-Host "Error: JSONL file not found: $JsonlFile" -ForegroundColor Red
    exit 1
}

if (-not (Test-Path $DatabaseFile)) {
    Write-Host "Error: Database file not found: $DatabaseFile" -ForegroundColor Red
    exit 1
}

# Create backup if requested
if (-not $SkipBackup) {
    $timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
    $backupFile = $DatabaseFile -replace "\.db$", "_backup_${timestamp}.db"
    Write-Host "Creating database backup: $backupFile" -ForegroundColor Yellow
    Copy-Item -Path $DatabaseFile -Destination $backupFile -Force
}

# Generate a task ID if not provided
if (-not $TaskId) {
    $TaskId = "import_" + (Get-Date -Format "yyyyMMdd_HHmmss") + "_" + [System.IO.Path]::GetFileNameWithoutExtension($JsonlFile)
}

# Create temp directory if it doesn't exist
$tempDir = "temp"
if (-not (Test-Path $tempDir)) {
    New-Item -ItemType Directory -Path $tempDir -Force | Out-Null
}

# Create import SQL script
$sqlFile = "$tempDir/import_data.sql"

Write-Host "Preparing SQL import script..." -ForegroundColor Cyan

# Script header with task creation
@"
-- Enable foreign keys
PRAGMA foreign_keys = ON;

-- Begin transaction
BEGIN TRANSACTION;

-- Create task if it doesn't exist
INSERT OR IGNORE INTO tasks (id, url, max_depth, follow_subdomains, max_links, created_at, assigned_at, incentive_amount)
VALUES ('$TaskId', 'imported', 2, 1, 100, strftime('%s', 'now'), strftime('%s', 'now'), 0);

"@ | Out-File -FilePath $sqlFile -Encoding utf8

# Process the JSONL file
Write-Host "Processing JSONL file: $JsonlFile" -ForegroundColor Cyan
$lineCount = 0
$pageCount = 0
$domain = ""

# Function to escape single quotes in SQL
function Escape-SqlString {
    param([string]$str)
    return $str -replace "'", "''"
}

# Function to detect JavaScript dependencies
function Detect-JavaScriptDependency {
    param([string]$html)
    
    $isJsDependent = $false
    $reasons = @()
    
    # Check for noscript tags with warnings
    if ($html -match '<noscript>.*?(javascript|enable|browser).*?</noscript>') {
        $isJsDependent = $true
        $reasons += "noscript warning found"
    }
    
    # Check for common JS frameworks - improved patterns
    if ($html -match '(ember[-\.](data|cli|engines|app)|ember\.js)') {
        $isJsDependent = $true
        $reasons += "Ember.js framework detected"
    }
    
    if ($html -match '(react[-\.](dom|app|query)|react\.js)') {
        $isJsDependent = $true
        $reasons += "React framework detected"
    }
    
    if ($html -match '(angular[-\.](core|material|router)|angular\.js)') {
        $isJsDependent = $true
        $reasons += "Angular framework detected"
    }
    
    if ($html -match '(vue[-\.](router|cli|store)|vue\.js)') {
        $isJsDependent = $true
        $reasons += "Vue.js framework detected"
    }
    
    # Check for script assets with source containing bundle or chunk
    if ($html -match '<script[^>]*src=["''][^"'']*bundle[^"'']*["'']') {
        $isJsDependent = $true
        $reasons += "JavaScript bundle detected"
    }
    
    # Check for modern build tools like webpack
    if ($html -match '(webpack|parcel|rollup|browserify)') {
        $isJsDependent = $true
        $reasons += "Modern JS build tools detected"
    }
    
    # Check for Single Page Application (SPA) routes
    if ($html -match '(data-route|data-page|ui-view|ng-view|router-view|v-view)') {
        $isJsDependent = $true
        $reasons += "SPA routing detected"
    }
    
    return @{
        IsDependent = $isJsDependent
        Reasons = ($reasons -join ", ")
    }
}

# Function to extract links from HTML
function Extract-Links {
    param([string]$html)
    
    $links = @()
    $pattern = '<a\s+[^>]*href\s*=\s*[''"]([^''"]+)[''"][^>]*>'
    $matches = [regex]::Matches($html, $pattern)
    
    foreach ($match in $matches) {
        $href = $match.Groups[1].Value.Trim()
        if ($href -and -not $href.StartsWith('#') -and $href -ne '/' -and $href -ne 'javascript:void(0)') {
            $links += $href
        }
    }
    
    return $links
}

try {
    $lines = Get-Content $JsonlFile
    $lineCount = 0
    
    foreach ($line in $lines) {
        $lineCount++
        try {
            # Convert JSON
            $page = $line | ConvertFrom-Json
            
            # Skip if missing required fields
            if (-not $page.url) {
                Write-Host "  Skipping line $lineCount - missing URL" -ForegroundColor Yellow
                continue
            }
            
            # Handle different JSONL formats
            $htmlContent = ""
            if ($page.html) {
                $htmlContent = $page.html
            } elseif ($page.body) {
                $htmlContent = $page.body
            } else {
                Write-Host "  Skipping line $lineCount - missing HTML content" -ForegroundColor Yellow
                continue
            }
            
            # Extract data
            $url = Escape-SqlString $page.url
            $domain = [System.Uri]::new($page.url).Host
            $status = if ($page.status) { 
                $page.status 
            } elseif ($page.status_code) { 
                $page.status_code 
            } else { 
                200 
            }
            
            $contentType = if ($page.content_type) { 
                Escape-SqlString $page.content_type 
            } else { 
                'text/html; charset=utf-8' 
            }
            
            $size = if ($page.size) { 
                $page.size 
            } else { 
                $htmlContent.Length 
            }
            
            # Extract title from HTML
            $title = ""
            if ($htmlContent -match '<title[^>]*>(.*?)</title>') {
                $title = Escape-SqlString $matches[1]
            }
            
            # Store HTML content securely
            $html = Escape-SqlString $htmlContent
            
            # Detect JavaScript dependency
            $jsDependency = Detect-JavaScriptDependency -html $htmlContent
            $isJsDependentInt = if ($jsDependency.IsDependent) { 1 } else { 0 }
            $jsReasons = Escape-SqlString $jsDependency.Reasons
            
            # Extract links
            $links = Extract-Links -html $htmlContent
            $linksJson = if ($links.Count -gt 0) { 
                $linksJsonStr = ConvertTo-Json $links -Compress
                Escape-SqlString $linksJsonStr
            } else { 
                "[]" 
            }
            
            # Create insert statement with upsert logic
            $sql = @"
-- Insert page: $url
INSERT OR REPLACE INTO crawled_pages (
    task_id, url, domain, status, content_type, title, 
    size, html, is_javascript_dependent, javascript_dependency_reasons, extracted_links
) VALUES (
    '$TaskId', '$url', '$domain', $status, '$contentType', '$title',
    $size, '$html', $isJsDependentInt, '$jsReasons', '$linksJson'
);

"@
            $sql | Out-File -FilePath $sqlFile -Append -Encoding utf8
            $pageCount++
        }
        catch {
            Write-Host "  Error processing line $($lineCount): $($_.Exception.Message)" -ForegroundColor Red
        }
    }
}
catch {
    Write-Host "Error: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}

# Finalize the SQL script
@"
-- Create or update crawl_result entry
INSERT OR REPLACE INTO crawl_results (
    task_id, domain, status, pages_count, pages, total_size, start_time, end_time
)
VALUES (
    '$TaskId',
    '$domain',
    'COMPLETED',
    $pageCount,
    '[]',
    (SELECT SUM(size) FROM crawled_pages WHERE task_id = '$TaskId'),
    strftime('%s', 'now') - 3600,
    strftime('%s', 'now')
);

-- Commit transaction
COMMIT;

-- Analyze for performance
ANALYZE;
"@ | Out-File -FilePath $sqlFile -Append -Encoding utf8

# Run the SQL script
Write-Host "Importing $pageCount pages into database..." -ForegroundColor Cyan
sqlite3 $DatabaseFile ".read $sqlFile"

# Verify import
$importedCount = sqlite3 $DatabaseFile "SELECT COUNT(*) FROM crawled_pages WHERE task_id = '$TaskId'"

# Display import results
Write-Host "`nImport complete!" -ForegroundColor Green
Write-Host "Processed $lineCount lines from JSONL file" -ForegroundColor Gray
Write-Host "Imported $importedCount pages into database" -ForegroundColor Gray

# Clean up temporary files
if (Test-Path $sqlFile) {
    Remove-Item -Path $sqlFile -Force
}
Write-Host "Cleaned up temporary files" -ForegroundColor Gray

# Offer to run analysis
$runAnalysis = Read-Host "Would you like to analyze the imported data? (y/n)"
if ($runAnalysis -eq "y") {
    Write-Host "`nRunning database analysis..." -ForegroundColor Cyan
    # Run the db_manager.ps1 script with analyze action
    & "$PSScriptRoot\db_manager.ps1" -DatabaseFile $DatabaseFile -Action analyze
} else {
    # Just show simple domain statistics
    $showStats = Read-Host "Would you like to view the basic domain statistics for imported data? (y/n)"
    if ($showStats -eq "y") {
        # Run a simplified domain statistics query
        sqlite3 $DatabaseFile -header -column "
            SELECT 
                domain,
                COUNT(*) AS pages,
                ROUND(AVG(size)/1024, 2) AS avg_size_kb,
                ROUND(SUM(size)/(1024*1024), 2) AS total_size_mb
            FROM 
                crawled_pages
            GROUP BY 
                domain
        "
    }
}

Write-Host "`nImport and analysis complete!" -ForegroundColor Green 