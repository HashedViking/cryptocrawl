# Enhanced Database Management wrapper script
param (
    [Parameter(Mandatory=$false)]
    [string]$DatabaseFile = "data/crawler.db"
)

# Check if the database file exists
if (-not (Test-Path $DatabaseFile)) {
    Write-Host "Database file not found. Initializing new database..." -ForegroundColor Yellow
    
    # Initialize the database with our schema
    .\db_manager.ps1 -DatabaseFile $DatabaseFile -Action init
    
    Write-Host "Database initialized successfully." -ForegroundColor Green
    exit
}

# Create a timestamp for the backup
$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"

# Backup the database
Write-Host "Creating backup of database..." -ForegroundColor Yellow
.\db_manager.ps1 -DatabaseFile $DatabaseFile -Action backup

# Analyze current database state
Write-Host "`nAnalyzing current database state..." -ForegroundColor Yellow
.\db_manager.ps1 -DatabaseFile $DatabaseFile -Action analyze

# Ask if user wants to perform database maintenance
$performMaintenance = Read-Host "`nDo you want to perform database maintenance (optimization, repair)? (y/n)"

if ($performMaintenance -eq "y") {
    # Run optimization
    Write-Host "`nPerforming database optimization..." -ForegroundColor Yellow
    .\db_manager.ps1 -DatabaseFile $DatabaseFile -Action optimize
    
    # Check if repair is needed
    $integrityCheck = sqlite3 $DatabaseFile "PRAGMA integrity_check;"
    if ($integrityCheck -ne "ok") {
        $performRepair = Read-Host "Database integrity issues detected. Do you want to repair the database? (y/n)"
        
        if ($performRepair -eq "y") {
            Write-Host "`nRepairing database..." -ForegroundColor Yellow
            .\db_manager.ps1 -DatabaseFile $DatabaseFile -Action repair
        }
    }
}

# Ask if user wants to migrate data
$migrateData = Read-Host "`nDo you want to migrate data from old format to new format? (y/n)"

if ($migrateData -eq "y") {
    Write-Host "`nMigrating data..." -ForegroundColor Yellow
    .\db_manager.ps1 -DatabaseFile $DatabaseFile -Action migrate
}

# Final analysis after all operations
Write-Host "`nFinal database analysis:" -ForegroundColor Cyan
.\db_manager.ps1 -DatabaseFile $DatabaseFile -Action analyze

Write-Host "`nDatabase enhancement completed!" -ForegroundColor Green 