# Cryptocrawl Database Management

This directory contains scripts and tools for managing the Cryptocrawl database.

## Database Structure

The Cryptocrawl database uses SQLite3 and consists of the following tables:

- `tasks`: Stores crawl tasks and their parameters
- `crawl_results`: Stores summary data for completed crawls
- `crawled_pages`: Stores the full content of crawled pages
- `crawl_reports`: Stores analysis reports for crawls
- `wallet_history`: Tracks incentive payments for crawl tasks

## Setup Instructions

1. Ensure SQLite3 is installed and available in your PATH
2. Run the initialization script: `.\db_manager.ps1 -Action init`

## Database Management Scripts

### db_manager.ps1

The main database management script that provides various actions:

```powershell
.\db_manager.ps1 -Action <action> -DatabaseFile <path> -Force
```

Available actions:

- `init`: Initialize a new database or reset an existing one
- `backup`: Create a timestamped backup of the database
- `optimize`: Run vacuum and optimization on the database
- `migrate`: Migrate data from old format to new format
- `repair`: Attempt to repair a corrupted database
- `analyze`: Generate a detailed analysis of the database contents

### enhance_db.ps1

A wrapper script that provides a user-friendly workflow for database maintenance:

```powershell
.\enhance_db.ps1 -DatabaseFile <path>
```

This script:
1. Creates a backup of the database
2. Analyzes the current state
3. Offers to perform optimization and repair if needed
4. Offers to migrate data from old format to new format
5. Provides a final analysis report

### import_jsonl.ps1

A script for importing crawled data from JSONL files:

```powershell
.\import_jsonl.ps1 -JsonlFile <path> -DatabaseFile <path> -TaskId <id> -CreateBackup
```

This script:
1. Reads a JSONL file containing crawled pages
2. Extracts URLs, content, and metadata
3. Detects JavaScript dependencies
4. Imports the data into the database
5. Offers to analyze the imported data

## Database Views

The database includes the following views:

- `v_crawled_pages`: Simplified view of crawled pages
- `v_js_dependency`: Analysis of JavaScript dependency rates by domain

## SQL Scripts

Original SQL scripts used by the database management tools are stored in the `scripts` directory.

## Common Workflows

### Initializing a Fresh Database

```powershell
.\db_manager.ps1 -Action init
```

### Importing Crawl Results

```powershell
.\import_jsonl.ps1 -JsonlFile "data/crawls/example.jsonl" -DatabaseFile "data/crawler.db"
```

### Analyzing the Database

```powershell
.\db_manager.ps1 -Action analyze
```

### Optimizing the Database

```powershell
.\db_manager.ps1 -Action optimize
```

### Complete Database Maintenance

```powershell
.\enhance_db.ps1
``` 