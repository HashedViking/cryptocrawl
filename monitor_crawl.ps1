# Monitor crawler performance
param (
    [Parameter(Mandatory=$true)]
    [string]$Url,
    
    [Parameter(Mandatory=$false)]
    [int]$MaxDepth = 2,
    
    [Parameter(Mandatory=$false)]
    [int]$MaxLinks = 20,
    
    [Parameter(Mandatory=$false)]
    [switch]$UseHeadlessChrome,
    
    [Parameter(Mandatory=$false)]
    [switch]$FollowSubdomains,
    
    [Parameter(Mandatory=$false)]
    [string]$OutDir = "data/crawls"
)

# Create output directory if it doesn't exist
if (-not (Test-Path $OutDir)) {
    New-Item -ItemType Directory -Path $OutDir -Force | Out-Null
}

# Generate timestamped filenames
$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$outBase = $Url -replace "https?://([^/]+).*", '$1'
$outJsonl = "$OutDir/${outBase}_${timestamp}.jsonl"
$logFile = "$OutDir/crawl_log_${timestamp}.txt"
$metricsFile = "$OutDir/metrics_${timestamp}.csv"

# Show crawl parameters
Write-Host "Starting crawl with the following parameters:" -ForegroundColor Cyan
Write-Host "  URL: $Url"
Write-Host "  Max Depth: $MaxDepth"
Write-Host "  Max Links: $MaxLinks"
Write-Host "  Follow Subdomains: $FollowSubdomains"
Write-Host "  Headless Chrome: $UseHeadlessChrome"
Write-Host "  Output: $outJsonl"
Write-Host "  Log: $logFile"
Write-Host "  Metrics: $metricsFile"
Write-Host ""

# Prepare metrics file headers
"Timestamp,Memory_MB,CPU_Percent,Pages_Crawled,Data_Size_KB,Elapsed_Seconds" | Out-File -FilePath $metricsFile -Encoding utf8

# Build the crawl command
$headlessParam = if ($UseHeadlessChrome) { "--use-headless-chrome" } else { "" }
$subdomainsParam = if ($FollowSubdomains) { "--follow-subdomains" } else { "" }

$cmd = "cargo run --bin cryptocrawl-crawler -- crawl-crates --max-depth $MaxDepth $subdomainsParam --max-links $MaxLinks $headlessParam --output $outJsonl"

# Start the crawl process
Write-Host "Starting crawl process..." -ForegroundColor Green
$process = Start-Process -FilePath "powershell.exe" -ArgumentList "-Command $cmd" -PassThru -RedirectStandardOutput $logFile -WindowStyle Hidden

Write-Host "Monitoring crawl process (PID: $($process.Id))..." -ForegroundColor Yellow
Write-Host "Press Ctrl+C to stop monitoring (crawl will continue in background)" -ForegroundColor Yellow
Write-Host ""

# Start time for elapsed calculation
$startTime = Get-Date

# Monitor the crawl process
$monitoring = $true
$lastPageCount = 0
$lastDataSize = 0

while ($monitoring) {
    try {
        # Check if process is still running
        if ($process.HasExited) {
            Write-Host "Crawl process has exited with code: $($process.ExitCode)" -ForegroundColor Yellow
            $monitoring = $false
            break
        }
        
        # Get process metrics
        $procInfo = Get-Process -Id $process.Id -ErrorAction SilentlyContinue
        if (-not $procInfo) {
            Write-Host "Process no longer exists" -ForegroundColor Red
            $monitoring = $false
            break
        }
        
        $memoryMB = [math]::Round($procInfo.WorkingSet64 / 1MB, 2)
        $cpuPercent = [math]::Round(($procInfo.CPU / (Get-Date).Subtract($startTime).TotalSeconds) * 100, 2)
        
        # Get crawl statistics from log
        $logContent = Get-Content $logFile -Tail 20 -ErrorAction SilentlyContinue
        $pageCount = 0
        $dataSize = 0
        
        foreach ($line in $logContent) {
            if ($line -match "Crawled (\d+) pages so far") {
                $pageCount = [int]$matches[1]
            }
            elseif ($line -match "pages, (\d+) bytes total") {
                $dataSize = [int]$matches[1]
            }
        }
        
        # Use the last known values if we couldn't extract from logs
        if ($pageCount -eq 0) { $pageCount = $lastPageCount }
        if ($dataSize -eq 0) { $dataSize = $lastDataSize }
        
        $lastPageCount = $pageCount
        $lastDataSize = $dataSize
        
        # Calculate elapsed time
        $elapsed = (Get-Date).Subtract($startTime).TotalSeconds
        
        # Record metrics
        "$((Get-Date).ToString('yyyy-MM-dd HH:mm:ss')),$memoryMB,$cpuPercent,$pageCount,$($dataSize/1KB),$elapsed" | Out-File -FilePath $metricsFile -Append -Encoding utf8
        
        # Display current status
        Clear-Host
        Write-Host "Crawl Status:" -ForegroundColor Cyan
        Write-Host "  Elapsed time: $([math]::Round($elapsed, 2)) seconds"
        Write-Host "  Memory usage: $memoryMB MB"
        Write-Host "  CPU usage: $cpuPercent%"
        Write-Host "  Pages crawled: $pageCount"
        Write-Host "  Data size: $([math]::Round($dataSize/1KB, 2)) KB"
        Write-Host ""
        Write-Host "Recent logs:" -ForegroundColor Cyan
        Get-Content $logFile -Tail 10 | ForEach-Object { Write-Host "  $_" }
        
        # Wait before checking again
        Start-Sleep -Seconds 5
    }
    catch {
        Write-Host "Error monitoring process: $($_.Exception.Message)" -ForegroundColor Red
        $monitoring = $false
    }
}

# Wait for process to complete if it's still running
if (-not $process.HasExited) {
    Write-Host "Waiting for crawl process to complete..." -ForegroundColor Yellow
    $process.WaitForExit()
}

# Display final metrics
$totalTime = (Get-Date).Subtract($startTime).TotalSeconds
$avgPagesPerMin = if ($lastPageCount -gt 0 -and $totalTime -gt 0) {
    [math]::Round(($lastPageCount / $totalTime) * 60, 2) 
} else { 
    0 
}

Write-Host "`nCrawl Completed!" -ForegroundColor Green
Write-Host "Total time: $([math]::Round($totalTime / 60, 2)) minutes"
Write-Host "Total pages crawled: $lastPageCount"
Write-Host "Total data size: $([math]::Round($lastDataSize/1KB, 2)) KB"
Write-Host "Average crawl rate: $avgPagesPerMin pages per minute"
Write-Host "Crawl results saved to: $outJsonl"
Write-Host "Log file: $logFile"
Write-Host "Metrics file: $metricsFile"

# Check if the process exited with error
if ($process.ExitCode -ne 0) {
    Write-Host "Crawl process exited with error code: $($process.ExitCode)" -ForegroundColor Red
}

# Offer to run analysis on the crawled data
Write-Host "`nDo you want to import the crawled data into the database for analysis? (y/n)" -ForegroundColor Cyan
$answer = Read-Host
if ($answer -eq "y") {
    Write-Host "Importing crawled data into database..." -ForegroundColor Yellow
    $importCmd = ".\import_jsonl.ps1 -JsonlFile '$outJsonl'"
    Invoke-Expression $importCmd
    
    Write-Host "`nDo you want to analyze the crawl metrics? (y/n)" -ForegroundColor Cyan
    $answer = Read-Host
    if ($answer -eq "y") {
        Write-Host "Analyzing crawl metrics..." -ForegroundColor Yellow
        $analyzeCmd = ".\analyze_metrics.ps1 -MetricsFile '$metricsFile'"
        Invoke-Expression $analyzeCmd
    }
} 