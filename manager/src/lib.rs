pub mod db;
pub mod models;
pub mod api;
pub mod evaluator;
pub mod solana;

use anyhow::Result;

// Re-export important types
pub use models::{Task, TaskStatus, CrawlReport};
pub use solana::SolanaIntegration; 