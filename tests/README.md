# CryptoCrawl Tests

This directory contains integration tests for the CryptoCrawl system.

## Integration Tests

The integration tests verify the full system workflow by testing the interaction between multiple components.

### Available Tests

- **integration_test**: Tests the complete workflow from manager startup to crawler task execution

### Running Tests

From the workspace root directory:

```bash
# Run the integration test
cargo run --bin integration_test
```

### Test Workflow

The integration test performs these steps:

1. Starts the manager service
2. Starts the crawler service
3. Registers the crawler with the manager
4. Creates a crawl task for https://crates.io/
5. Waits for the crawler to complete the task
6. Verifies the results in both databases

### Setup Requirements

Before running the integration tests:

1. Make sure both SQLite and Rust are properly installed
2. Ensure no other services are using the test ports (8001 and 3001)
3. Kill any existing instances of the manager or crawler:
   ```bash
   taskkill /f /im cryptocrawl-manager.exe
   taskkill /f /im cryptocrawl-crawler.exe
   ```

### Expected Results

A successful test run should:
- Complete without errors
- Create database files with proper data
- Show matching task IDs between the manager and crawler databases

For more details, see `integration_test_summary.md`. 