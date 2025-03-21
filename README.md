# CryptoCrawl

A decentralized web crawling platform powered by Solana blockchain.

## Overview

CryptoCrawl is a distributed web crawling system that uses blockchain technology to incentivize participants. The system consists of two main components:

1. **Manager**: Coordinates crawl tasks, verifies results, and handles incentive distribution.
2. **Crawler**: Performs the actual web crawling, submits reports, and earns incentives.

## Prerequisites

- Rust 1.56.0 or higher
- SQLite 3
- Solana CLI (for blockchain interaction)
- Ollama (for AI-powered verification)

## API Documentation with daipendency

The CryptoCrawl project uses `daipendency` (v1.2.5) for extracting API documentation from crates to make it easier to understand and work with dependencies.

### Using the CLI Tool

You can extract API documentation using the standalone binary:

```bash
# Extract API docs for any crate
cargo run --bin extract_api_docs -- <crate-name>

# Examples
cargo run --bin extract_api_docs -- anyhow
cargo run --bin extract_api_docs -- spider
```

The tool will output the documentation in a format that is friendly for large language models, including detailed information about the crate's structs, functions, enums, and traits.

### Using from Manager

The manager application includes a command for generating API documentation:

```bash
cargo run --bin cryptocrawl-manager -- get-api-docs <crate-name>
```

### Implementation Details

The API documentation extraction uses the `daipendency` CLI under the hood, which formats the output in a way that preserves the structure and context of the documentation. The implementation can be found in:

- `tools/src/bin/extract_api_docs.rs` - Standalone binary for API extraction
- `manager/src/evaluator.rs` - Integration with the manager application

## Project Structure

```
cryptocrawl/
├── manager/         # Manager component
├── crawler/         # Crawler component
├── tools/           # Utility tools
├── data/            # Data directories (created at runtime)
│   ├── manager/     # Manager database and logs
│   └── crawler/     # Crawler database and logs
├── cache/           # Cache for API documentation
├── keys/            # Solana keypair storage
├── config/          # Configuration files
└── logs/            # Log files
```

## Setup

1. Clone the repository:
   ```
   git clone https://github.com/yourusername/cryptocrawl.git
   cd cryptocrawl
   ```

2. Run the setup script to create the necessary directories:
   ```
   cargo run --bin setup
   ```

3. Build both components:
   ```
   cargo build
   ```

## Running the Manager

Start the manager server with:

```
cargo run --bin manager -- server --db-path data/manager/manager.db
```

Additional options:
- `--host <HOST>`: Bind to a specific host (default: 127.0.0.1)
- `--port <PORT>`: Bind to a specific port (default: 8000)
- `--log-level <LEVEL>`: Set log level (default: info)
- `--keypair-path <PATH>`: Path to Solana keypair (default: keys/manager_wallet.json)
- `--rpc-endpoint <URL>`: Solana RPC endpoint (default: https://api.devnet.solana.com)
- `--ollama-host <URL>`: Ollama API host (default: http://localhost:11434)
- `--ollama-model <MODEL>`: Ollama model to use (default: llama3)

Create a new crawl task:

```
cargo run --bin manager -- create-task https://example.com --max-depth 2 --follow-subdomains
```

## Running the Crawler

Start a crawler that continuously polls for new tasks:

```
cargo run --bin crawler -- --db-path data/crawler/crawler.db --manager-url http://localhost:8000
```

Additional options:
- `--log-level <LEVEL>`: Set log level (default: info)
- `--keypair-path <PATH>`: Path to Solana keypair (default: keys/crawler_wallet.json)
- `--rpc-endpoint <URL>`: Solana RPC endpoint (default: https://api.devnet.solana.com)
- `--poll-interval <SECONDS>`: Time between polls for new tasks (default: 60)
- `--config <PATH>`: Path to configuration file

## Configuration

Both the manager and crawler support JSON configuration files. Example:

### Manager Config (config/manager.json)
```json
{
    "db_path": "data/manager/manager.db",
    "log_level": "info",
    "keypair_path": "keys/manager_wallet.json",
    "rpc_endpoint": "https://api.devnet.solana.com",
    "program_id": "CrawLY3R5pzRHE1b31TvhG8zX1CRkFxc1xECDZ97ihkUS",
    "ollama_host": "http://localhost:11434",
    "ollama_model": "llama3",
    "server": {
        "host": "127.0.0.1",
        "port": 8000
    }
}
```

### Crawler Config (config/crawler.json)
```json
{
    "db_path": "data/crawler/crawler.db",
    "log_level": "info",
    "keypair_path": "keys/crawler_wallet.json",
    "rpc_endpoint": "https://api.devnet.solana.com",
    "program_id": "CrawLY3R5pzRHE1b31TvhG8zX1CRkFxc1xECDZ97ihkUS",
    "manager_url": "http://localhost:8000",
    "poll_interval": 60
}
```

## API Documentation

The manager provides an API for:
- Task assignment
- Report submission
- Verification
- Incentive distribution

API documentation can be accessed at `http://localhost:8000/api/docs`.

## License

MIT License 