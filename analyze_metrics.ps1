# Analyze crawl metrics script
param (
    [Parameter(Mandatory=$true)]
    [string]$MetricsFile
)

# Verify metrics file exists
if (-not (Test-Path $MetricsFile)) {
    Write-Host "Error: Metrics file not found: $MetricsFile" -ForegroundColor Red
    exit 1
}

# Import the metrics data
Write-Host "Importing metrics data from: $MetricsFile" -ForegroundColor Cyan
$metrics = Import-Csv -Path $MetricsFile

# Check if we have data
if ($metrics.Count -eq 0) {
    Write-Host "No metrics data found in the file." -ForegroundColor Yellow
    exit 0
}

# Prepare stats
$totalDuration = [double]($metrics[-1].Elapsed_Seconds) - [double]($metrics[0].Elapsed_Seconds)
$avgMemory = ($metrics | Measure-Object -Property Memory_MB -Average).Average
$peakMemory = ($metrics | Measure-Object -Property Memory_MB -Maximum).Maximum
$avgCpu = ($metrics | Measure-Object -Property CPU_Percent -Average).Average
$peakCpu = ($metrics | Measure-Object -Property CPU_Percent -Maximum).Maximum
$finalPageCount = [int]$metrics[-1].Pages_Crawled
$finalDataSize = [double]$metrics[-1].Data_Size_KB

# Calculate crawl rates
$crawlRatePerMinute = if ($totalDuration -gt 0) { 
    [math]::Round(($finalPageCount / $totalDuration) * 60, 2) 
} else { 
    0 
}

$dataSizeRatePerMinute = if ($totalDuration -gt 0) { 
    [math]::Round(($finalDataSize / $totalDuration) * 60, 2) 
} else { 
    0 
}

# Display summary
Write-Host "`nCrawl Metrics Summary:" -ForegroundColor Green
Write-Host "----------------------" -ForegroundColor Green

Write-Host "Duration: $([math]::Round($totalDuration / 60, 2)) minutes"
Write-Host "Pages Crawled: $finalPageCount"
Write-Host "Data Size: $([math]::Round($finalDataSize / 1024, 2)) MB"
Write-Host "Crawl Rate: $crawlRatePerMinute pages per minute"
Write-Host "Data Collection Rate: $dataSizeRatePerMinute KB per minute"
Write-Host "`nResource Usage:" -ForegroundColor Cyan
Write-Host "Average Memory: $([math]::Round($avgMemory, 2)) MB"
Write-Host "Peak Memory: $([math]::Round($peakMemory, 2)) MB"
Write-Host "Average CPU: $([math]::Round($avgCpu, 2))%"
Write-Host "Peak CPU: $([math]::Round($peakCpu, 2))%"

# Check if there were any performance bottlenecks
Write-Host "`nPerformance Analysis:" -ForegroundColor Cyan
if ($peakMemory -gt 1000) {
    Write-Host "⚠️ High memory usage detected. Consider optimizing memory usage." -ForegroundColor Yellow
}

if ($peakCpu -gt 90) {
    Write-Host "⚠️ High CPU usage detected. The crawl may have been CPU-bound." -ForegroundColor Yellow
}

if ($crawlRatePerMinute -lt 10) {
    Write-Host "⚠️ Low crawl rate detected. Consider investigating network or parsing bottlenecks." -ForegroundColor Yellow
}

# Calculate memory and CPU over time for trend analysis
$memoryTrend = [System.Collections.ArrayList]::new()
$cpuTrend = [System.Collections.ArrayList]::new()
$pagesTrend = [System.Collections.ArrayList]::new()

$windowSize = [Math]::Min(5, $metrics.Count)
for ($i = 0; $i -lt $metrics.Count - $windowSize; $i += $windowSize) {
    $slice = $metrics[$i..($i+$windowSize-1)]
    $avgMemSlice = ($slice | Measure-Object -Property Memory_MB -Average).Average
    $avgCpuSlice = ($slice | Measure-Object -Property CPU_Percent -Average).Average
    $pageCount = [int]$slice[-1].Pages_Crawled - [int]$slice[0].Pages_Crawled
    
    $memoryTrend.Add([math]::Round($avgMemSlice, 2)) | Out-Null
    $cpuTrend.Add([math]::Round($avgCpuSlice, 2)) | Out-Null
    $pagesTrend.Add($pageCount) | Out-Null
}

# Display trends
Write-Host "`nPerformance Trends:" -ForegroundColor Cyan
Write-Host "Memory Usage Trend: $([string]::Join(" → ", $memoryTrend)) MB"
Write-Host "CPU Usage Trend: $([string]::Join(" → ", $cpuTrend))%"
Write-Host "Pages Crawled per Window: $([string]::Join(" → ", $pagesTrend))"

# Analyze if there was a slowdown
if ($pagesTrend.Count -gt 2) {
    $firstHalf = $pagesTrend[0..([Math]::Floor($pagesTrend.Count/2)-1)]
    $secondHalf = $pagesTrend[[Math]::Floor($pagesTrend.Count/2)..($pagesTrend.Count-1)]
    
    $avgFirstHalf = ($firstHalf | Measure-Object -Average).Average
    $avgSecondHalf = ($secondHalf | Measure-Object -Average).Average
    
    if ($avgSecondHalf -lt $avgFirstHalf * 0.7) {
        Write-Host "`n⚠️ Significant slowdown detected in the second half of the crawl." -ForegroundColor Yellow
        Write-Host "   Average pages per window in first half: $([math]::Round($avgFirstHalf, 2))" -ForegroundColor Yellow
        Write-Host "   Average pages per window in second half: $([math]::Round($avgSecondHalf, 2))" -ForegroundColor Yellow
    }
}

# Generate recommendations
Write-Host "`nRecommendations:" -ForegroundColor Green

if ($peakMemory -gt 1000) {
    Write-Host "- Consider adding pagination to reduce memory usage" -ForegroundColor White
}

if ($crawlRatePerMinute -lt 10) {
    Write-Host "- Increase parallelism by using multiple crawl threads" -ForegroundColor White
}

if ($peakCpu -gt 90) {
    Write-Host "- Consider reducing HTML parsing complexity or using a more efficient parser" -ForegroundColor White
}

if ($finalPageCount -lt 5) {
    Write-Host "- The crawl gathered very few pages. Consider checking robots.txt restrictions or increasing max_depth" -ForegroundColor White
}

# Export to SQLite for further analysis
Write-Host "`nWould you like to export metrics to SQLite for further analysis? (y/n)" -ForegroundColor Cyan
$answer = Read-Host

if ($answer -eq "y") {
    $dbFile = "data/metrics.db"
    
    Write-Host "Exporting metrics to SQLite database: $dbFile" -ForegroundColor Yellow
    
    # Create metrics database if it doesn't exist
    if (-not (Test-Path $dbFile)) {
        $createTableSql = @"
CREATE TABLE metrics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT,
    memory_mb REAL,
    cpu_percent REAL,
    pages_crawled INTEGER,
    data_size_kb REAL,
    elapsed_seconds REAL,
    crawl_session TEXT
);

CREATE INDEX idx_metrics_crawl_session ON metrics(crawl_session);
"@
        $createTableSql | sqlite3 $dbFile
    }
    
    # Generate a session identifier from the metrics file name
    $sessionId = [System.IO.Path]::GetFileNameWithoutExtension($MetricsFile)
    
    # Create a temporary file for the insert statements
    $insertSql = "BEGIN TRANSACTION;`n"
    foreach ($metric in $metrics) {
        $insertSql += "INSERT INTO metrics (timestamp, memory_mb, cpu_percent, pages_crawled, data_size_kb, elapsed_seconds, crawl_session) VALUES ('$($metric.Timestamp)', $($metric.Memory_MB), $($metric.CPU_Percent), $($metric.Pages_Crawled), $($metric.Data_Size_KB), $($metric.Elapsed_Seconds), '$sessionId');`n"
    }
    $insertSql += "COMMIT;"
    
    # Execute the insert statements
    $insertSql | sqlite3 $dbFile
    
    Write-Host "Metrics exported to SQLite database successfully." -ForegroundColor Green
}

Write-Host "`nMetrics analysis complete!" -ForegroundColor Green 