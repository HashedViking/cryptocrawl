# CryptoCrawl Crawler

A decentralized web crawler with blockchain incentives built on Rust and Solana.

## Overview

The CryptoCrawl Crawler is part of the CryptoCrawl ecosystem, which incentivizes web crawling and data collection using blockchain technology. Crawlers receive tokens for completing crawling tasks and submitting the results to the CryptoCrawl network.

## Features

- Web crawling with configurable depth, link limits, and subdomain handling
- SQLite database for storing tasks and crawl results
- Solana blockchain integration for receiving incentives
- Web UI for monitoring and managing crawling tasks
- Command-line interface for direct crawling

## Installation

### Prerequisites

- Rust and Cargo (1.70.0 or newer)
- SQLite3
- Solana CLI tools (optional, for real blockchain integration)

### Building from Source

```bash
git clone https://github.com/yourusername/cryptocrawl.git
cd cryptocrawl/crawler
cargo build --release
```

The binary will be available at `target/release/cryptocrawl-crawler`.

## Usage

### Starting the UI

```bash
cargo run -- ui --host 127.0.0.1 --port 3000
```

This starts the web interface on http://127.0.0.1:3000, allowing you to monitor and manage crawling tasks.

### Command-line Crawling

```bash
cargo run -- crawl https://example.com --max-depth 3 --follow-subdomains
```

### Registering as a Crawler

```bash
cargo run -- register
```

## Configuration

You can configure the crawler using command-line options:

- `--db-path`: Path to the SQLite database file (default: `crawler.db`)
- `--log-level`: Logging level (default: `info`)
- `--client-id`: Custom client ID (default: auto-generated UUID)
- `--keypair-path`: Path to Solana keypair file (default: `wallet.json`)
- `--rpc-endpoint`: Solana RPC endpoint (default: `https://api.devnet.solana.com`)
- `--program-id`: CryptoCrawl program ID on Solana (default: `CrawLY3R5pzRHE1b31TvhG8zX1CRkFxc1xECDZ97ihkUS`)
- `--manager-pubkey`: Manager's public key for submitting reports (default: `5MxUVGwsu3VAfBCwGS5sMwKyL2Vt3WvVrYLmX1fMcbZS`)

## Architecture

The crawler consists of several key components:

- **Crawler**: Handles the web crawling logic using the spider-rs library
- **Database**: Manages task and result storage using SQLite
- **Solana Integration**: Handles blockchain interactions for receiving incentives
- **UI**: Provides a web interface for monitoring and control
- **Models**: Contains data structures used throughout the application

## Development

### Project Structure

```
crawler/
├── src/
│   ├── main.rs         # Entry point and CLI handling
│   ├── crawler.rs      # Web crawling implementation
│   ├── db.rs           # Database operations
│   ├── models.rs       # Data structures
│   ├── solana.rs       # Blockchain integration
│   └── ui.rs           # Web interface
├── Cargo.toml          # Dependencies and configuration
└── README.md           # Documentation
```

### Testing

```bash
cargo test
```

## License

This project is licensed under the MIT License - see the LICENSE file for details. 