# Cryptocrawl Database Management Script
param (
    [Parameter(Mandatory=$false)]
    [string]$DatabaseFile = "data/crawler.db",
    
    [Parameter(Mandatory=$false)]
    [ValidateSet("init", "backup", "optimize", "migrate", "repair", "analyze")]
    [string]$Action = "analyze",
    
    [Parameter(Mandatory=$false)]
    [switch]$Force
)

# Ensure the data directory exists
if (-not (Test-Path "data")) {
    New-Item -ItemType Directory -Path "data" -Force | Out-Null
    Write-Host "Created data directory" -ForegroundColor Green
}

# Create backup function
function Backup-Database {
    param (
        [string]$DbFile
    )
    
    $timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
    $backupFile = $DbFile -replace "\.db$", "_backup_${timestamp}.db"
    
    Write-Host "Creating backup of database at $backupFile..." -ForegroundColor Yellow
    Copy-Item $DbFile -Destination $backupFile -Force
    
    if (Test-Path $backupFile) {
        Write-Host "Backup created successfully!" -ForegroundColor Green
        return $backupFile
    } else {
        Write-Host "Failed to create backup!" -ForegroundColor Red
        return $null
    }
}

# Initialize database function
function Initialize-Database {
    param (
        [string]$DbFile,
        [switch]$Force
    )
    
    # Check if database already exists
    $dbExists = Test-Path $DbFile
    
    if ($dbExists -and -not $Force) {
        Write-Host "Database already exists. Use -Force to reinitialize." -ForegroundColor Yellow
        return
    }
    
    if ($dbExists) {
        $backupFile = Backup-Database -DbFile $DbFile
        if ($null -eq $backupFile) {
            return
        }
    }
    
    # Create/initialize the database
    Write-Host "Initializing database at $DbFile..." -ForegroundColor Yellow
    Get-Content "init_db.sql" | sqlite3 $DbFile
    
    # Verify initialization
    $tables = sqlite3 $DbFile ".tables"
    if ($tables -match "crawled_pages") {
        Write-Host "Database initialized successfully!" -ForegroundColor Green
    } else {
        Write-Host "Database initialization failed!" -ForegroundColor Red
        if ($dbExists -and $Force -and (Test-Path $backupFile)) {
            Write-Host "Restoring from backup..." -ForegroundColor Yellow
            Copy-Item $backupFile -Destination $DbFile -Force
        }
    }
}

# Optimize database function
function Optimize-Database {
    param (
        [string]$DbFile
    )
    
    # Create a backup first
    $backupFile = Backup-Database -DbFile $DbFile
    if ($null -eq $backupFile) {
        return
    }
    
    # Run optimization
    Write-Host "Optimizing database..." -ForegroundColor Yellow
    $optimizeCommands = @"
PRAGMA auto_vacuum = FULL;
PRAGMA optimize;
VACUUM;
ANALYZE;
"@
    
    $optimizeCommands | sqlite3 $DbFile
    
    # Compare sizes
    $originalSize = (Get-Item $backupFile).Length
    $newSize = (Get-Item $DbFile).Length
    $savedSpace = $originalSize - $newSize
    
    if ($savedSpace -gt 0) {
        $savedPercent = [math]::Round(($savedSpace / $originalSize) * 100, 2)
        Write-Host "Optimization complete! Saved $([math]::Round($savedSpace / 1KB, 2)) KB ($savedPercent%)" -ForegroundColor Green
    } else {
        Write-Host "Optimization complete, but no space saved." -ForegroundColor Yellow
    }
}

# Migrate data function (from old format to new)
function Migrate-Data {
    param (
        [string]$DbFile
    )
    
    # Create a backup first
    $backupFile = Backup-Database -DbFile $DbFile
    if ($null -eq $backupFile) {
        return
    }
    
    # Check if migration is needed
    $oldPageCount = sqlite3 $DbFile "SELECT COUNT(*) FROM crawl_results"
    $newPageCount = sqlite3 $DbFile "SELECT COUNT(*) FROM crawled_pages"
    
    if ($oldPageCount -eq 0) {
        Write-Host "No data to migrate (no records in crawl_results table)" -ForegroundColor Yellow
        return
    }
    
    if ($newPageCount -ge $oldPageCount) {
        Write-Host "Migration appears to have already been done." -ForegroundColor Yellow
        Write-Host "crawl_results: $oldPageCount, crawled_pages: $newPageCount" -ForegroundColor Gray
        
        $answer = Read-Host "Do you want to force migration? (y/n)"
        if ($answer -ne "y") {
            return
        }
    }
    
    # Run migration scripts
    Write-Host "Migrating data from old format to new format..." -ForegroundColor Yellow
    
    # Uncomment the migration section in init_db.sql by creating a temporary file
    $migrationSql = Get-Content "init_db.sql" -Raw
    $migrationSql = $migrationSql -replace '/\*\nINSERT OR IGNORE INTO crawled_pages', 'INSERT OR IGNORE INTO crawled_pages'
    $migrationSql = $migrationSql -replace 'p.key = .url. OR p.key = .html.;\n\*/', 'p.key = ''url'' OR p.key = ''html'';'
    $migrationSql | Out-File "temp_migration.sql" -Encoding utf8
    
    # Execute the migration SQL
    Get-Content "temp_migration.sql" | sqlite3 $DbFile
    
    # Clean up
    Remove-Item "temp_migration.sql" -Force
    
    # Verify migration
    $newCountAfter = sqlite3 $DbFile "SELECT COUNT(*) FROM crawled_pages"
    if ($newCountAfter -gt $newPageCount) {
        Write-Host "Migration completed successfully! Migrated $($newCountAfter - $newPageCount) records." -ForegroundColor Green
    } else {
        Write-Host "Migration did not add any new records." -ForegroundColor Yellow
    }
}

# Repair database function
function Repair-Database {
    param (
        [string]$DbFile
    )
    
    # Create a backup first
    $backupFile = Backup-Database -DbFile $DbFile
    if ($null -eq $backupFile) {
        return
    }
    
    Write-Host "Running database integrity check..." -ForegroundColor Yellow
    $integrityCheck = sqlite3 $DbFile "PRAGMA integrity_check;"
    
    if ($integrityCheck -eq "ok") {
        Write-Host "Database integrity check passed." -ForegroundColor Green
    } else {
        Write-Host "Database integrity issues found. Attempting repair..." -ForegroundColor Red
        
        # Export and reimport the database
        $tempDir = "temp_db_repair"
        if (-not (Test-Path $tempDir)) {
            New-Item -ItemType Directory -Path $tempDir -Force | Out-Null
        }
        
        # Export all tables
        $tables = (sqlite3 $DbFile ".tables") -split '\s+'
        $tables | ForEach-Object {
            $table = $_.Trim()
            if ($table -and -not $table.StartsWith("sqlite_") -and -not $table.StartsWith("v_")) {
                Write-Host "Exporting table $table..." -ForegroundColor Gray
                sqlite3 $DbFile ".mode insert" ".output $tempDir/$table.sql" "SELECT * FROM $table;"
            }
        }
        
        # Create a new database
        $newDb = "$DbFile.new"
        if (Test-Path $newDb) {
            Remove-Item $newDb -Force
        }
        
        # Initialize the new database
        Get-Content "init_db.sql" | sqlite3 $newDb
        
        # Import data into the new database
        Get-ChildItem $tempDir -Filter "*.sql" | ForEach-Object {
            Write-Host "Importing table $($_.BaseName)..." -ForegroundColor Gray
            Get-Content $_.FullName | sqlite3 $newDb
        }
        
        # Replace the old database with the new one
        Remove-Item $DbFile -Force
        Move-Item $newDb $DbFile -Force
        
        # Clean up
        Remove-Item $tempDir -Force -Recurse
        
        Write-Host "Database repair completed!" -ForegroundColor Green
    }
}

# Analyze database function
function Analyze-Database {
    param (
        [string]$DbFile
    )
    
    Write-Host "`nDatabase Analysis Report" -ForegroundColor Cyan
    Write-Host "=======================" -ForegroundColor Cyan
    
    # Check if database exists
    if (-not (Test-Path $DbFile)) {
        Write-Host "Database file not found at $DbFile" -ForegroundColor Red
        return
    }
    
    # Get database size
    $dbSize = (Get-Item $DbFile).Length
    Write-Host "Database Size: $([math]::Round($dbSize / 1KB, 2)) KB ($([math]::Round($dbSize / 1MB, 2)) MB)" -ForegroundColor White
    
    # Get table counts
    Write-Host "`nTable Statistics:" -ForegroundColor Yellow
    $tables = @(
        "tasks", 
        "crawl_results", 
        "crawled_pages", 
        "crawl_reports", 
        "wallet_history"
    )
    
    foreach ($table in $tables) {
        $count = sqlite3 $DbFile "SELECT COUNT(*) FROM $table"
        Write-Host "  $table`.`: $count records" -ForegroundColor White
    }
    
    # JavaScript dependency analysis
    Write-Host "`nJavaScript Dependency Analysis:" -ForegroundColor Yellow
    sqlite3 $DbFile -header -column "
        SELECT 
            domain, 
            total_pages, 
            js_dependent_pages,
            js_dependency_percentage || '%' as dependency_rate
        FROM 
            v_js_dependency
        ORDER BY 
            js_dependency_percentage DESC
        LIMIT 5
    "
    
    # Most linked pages
    Write-Host "`nTop Hub Pages (Most outgoing links):" -ForegroundColor Yellow
    sqlite3 $DbFile -header -column "
        SELECT 
            url, 
            domain,
            (SELECT COUNT(*) FROM json_each(extracted_links)) AS link_count
        FROM 
            crawled_pages
        ORDER BY 
            link_count DESC
        LIMIT 5
    "
    
    # Domain statistics
    Write-Host "`nDomain Statistics:" -ForegroundColor Yellow
    sqlite3 $DbFile -header -column "
        SELECT 
            domain,
            COUNT(*) AS pages,
            ROUND(AVG(size)/1024, 2) AS avg_size_kb,
            ROUND(SUM(size)/(1024*1024), 2) AS total_size_mb
        FROM 
            crawled_pages
        GROUP BY 
            domain
        ORDER BY 
            pages DESC
        LIMIT 5
    "
    
    # Database health check
    Write-Host "`nDatabase Health Check:" -ForegroundColor Yellow
    $integrityCheck = sqlite3 $DbFile "PRAGMA integrity_check;"
    if ($integrityCheck -eq "ok") {
        Write-Host "  Integrity Check: Passed" -ForegroundColor Green
    } else {
        Write-Host "  Integrity Check: Failed - Consider running Repair" -ForegroundColor Red
    }
    
    $foreignKeyCheck = sqlite3 $DbFile "PRAGMA foreign_keys = ON; PRAGMA foreign_key_check;"
    if ([string]::IsNullOrWhiteSpace($foreignKeyCheck)) {
        Write-Host "  Foreign Key Check: Passed" -ForegroundColor Green
    } else {
        Write-Host "  Foreign Key Check: Failed - Foreign key violations found" -ForegroundColor Red
        Write-Host "  Violations: $foreignKeyCheck" -ForegroundColor Gray
    }
}

# Main execution logic
switch ($Action) {
    "init" {
        Initialize-Database -DbFile $DatabaseFile -Force:$Force
    }
    "backup" {
        Backup-Database -DbFile $DatabaseFile
    }
    "optimize" {
        Optimize-Database -DbFile $DatabaseFile
    }
    "migrate" {
        Migrate-Data -DbFile $DatabaseFile
    }
    "repair" {
        Repair-Database -DbFile $DatabaseFile
    }
    "analyze" {
        Analyze-Database -DbFile $DatabaseFile
    }
}

Write-Host "`nDatabase management completed!" -ForegroundColor Cyan 