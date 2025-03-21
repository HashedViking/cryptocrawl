# CryptoCrawl Integration Test

This integration test verifies the full workflow of the CryptoCrawl system, including:

1. Starting the manager service
2. Starting the crawler service
3. Registering the crawler with the manager
4. Creating a crawl task
5. Waiting for the task to be completed
6. Verifying the results in the databases

## How to Run the Test

To run the integration test:

```bash
# Make sure no other instances are running
taskkill /f /im cryptocrawl-manager.exe
taskkill /f /im cryptocrawl-crawler.exe

# Run the test
cargo run --bin integration_test
```

## What the Test Does

1. **Cleanup**: Removes any existing database files to start with a clean state
2. **Start Manager**: Launches the manager service on port 8001
3. **Start Crawler**: Launches the crawler service on port 3001
4. **Registration**: Registers the crawler with the manager
5. **Task Creation**: Creates a new crawl task for https://crates.io/
6. **Task Execution**: Waits for the crawler to process the task
7. **Verification**: Checks both databases to ensure data was properly saved

## Expected Results

When successful, the test will:
- Create a manager database at `data/manager.db` with task data
- Create a crawler database at `crawler.db` with crawl results
- Verify that the task IDs match between the two databases
- Output confirmation that the workflow is working correctly

## Troubleshooting

- **Port conflicts**: If you see "Address already in use" errors, make sure no other instances are running
- **Database locked**: If you can't delete database files, make sure all services are terminated
- **Connection refused**: Increase the wait times in the test to give services more time to start up

## Database Structure

### Manager Database (data/manager.db)
- `tasks`: Contains all crawl tasks and their status

### Crawler Database (crawler.db)
- `crawl_results`: Contains the crawling results including HTML content
- `tasks`: Mirrors the tasks from the manager that were assigned to this crawler

You can query these databases directly to see more details:

```bash
# View task details in manager database
sqlite3 -header -column data/manager.db "SELECT * FROM tasks LIMIT 5;"

# View crawl results in crawler database
sqlite3 -header -column crawler.db "SELECT task_id, domain, status, pages_count FROM crawl_results LIMIT 5;"
``` 