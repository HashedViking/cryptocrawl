# CryptoCrawl

A blockchain incentivized web crawler Proof-of-Concept built with Rust and Solana.

## Overview

CryptoCrawl is a web crawler that uses blockchain technology to incentivize web crawling activities. The crawler collects web data and receives token incentives through the Solana blockchain.

### Key Components

1. **Web Crawler** - Built using [spider-rs](https://github.com/spider-rs/spider), a powerful and fast web crawler for Rust
2. **Blockchain Integration** - Using Solana for incentivization
3. **Crawler Reports** - Structured data about crawled websites

## Features

- Configurable web crawling with depth and concurrency controls
- Automatic collection of page data (URL, status, content type, size)
- Integration with Solana blockchain for incentives
- Reporting of crawl statistics

## Prerequisites

- Rust (1.70.0 or later)
- Cargo
- Internet connection
- (Optional) Solana CLI tools for advanced blockchain interactions

## Installation

1. Clone this repository:
```
git clone https://github.com/yourusername/cryptocrawl.git
cd cryptocrawl
```

2. Build the project:
```
cargo build --release
```

## Usage

### Basic Crawl

To crawl a website and simulate blockchain incentivization:

```
cargo run -- https://example.com
```

Replace `https://example.com` with the URL you want to crawl.

### Advanced Usage

You can customize the crawl behavior by modifying the code:

- Change max depth in `main.rs`:
  ```rust
  spider.set_max_depth(3); // Increase to crawl deeper
  ```

- Adjust concurrency:
  ```rust
  spider.set_concurrency(5); // Increase for faster crawling
  ```

## Project Structure

- `src/main.rs` - Main application code and crawler implementation
- `src/solana_integration.rs` - Solana blockchain integration code
- `src/crawl_data.rs` - Data structures for crawl reports

## Blockchain Integration

In this PoC:
- The crawler simulates submitting crawl reports to the Solana blockchain
- Incentives are calculated based on the crawl activity
- For a production version, a real Solana program would need to be deployed

## Future Enhancements

- Implement actual Solana smart contract for real incentivization
- Add distributed crawling capabilities
- Implement data quality validation
- Add more advanced crawling features (JavaScript rendering, etc.)
- Create a marketplace for crawl requests

## License

MIT

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request. 