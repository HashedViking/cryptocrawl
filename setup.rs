use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Result, Context};

/// Ensure a directory exists, creating it if necessary
fn ensure_dir_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        println!("Creating directory: {:?}", path);
        fs::create_dir_all(path)
            .context(format!("Failed to create directory {:?}", path))?;
    }
    Ok(())
}

/// Ensure a parent directory for a file exists
fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir_exists(parent)?;
    }
    Ok(())
}

/// Create default directories for the project
fn create_default_directories() -> Result<()> {
    // Create data directory
    let data_dir = PathBuf::from("data");
    ensure_dir_exists(&data_dir)?;
    
    // Create database directories
    let manager_db_dir = data_dir.join("manager");
    let crawler_db_dir = data_dir.join("crawler");
    ensure_dir_exists(&manager_db_dir)?;
    ensure_dir_exists(&crawler_db_dir)?;
    
    // Create keypair directories
    let keypair_dir = PathBuf::from("keys");
    ensure_dir_exists(&keypair_dir)?;
    
    // Create logs directory
    let logs_dir = PathBuf::from("logs");
    ensure_dir_exists(&logs_dir)?;
    
    println!("Created default directories structure");
    Ok(())
}

/// Create default configuration files
fn create_default_configs() -> Result<()> {
    // Create manager config
    let manager_config = r#"{
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
}"#;

    // Create crawler config
    let crawler_config = r#"{
    "db_path": "data/crawler/crawler.db",
    "log_level": "info",
    "keypair_path": "keys/crawler_wallet.json",
    "rpc_endpoint": "https://api.devnet.solana.com",
    "program_id": "CrawLY3R5pzRHE1b31TvhG8zX1CRkFxc1xECDZ97ihkUS",
    "manager_url": "http://127.0.0.1:8000"
}"#;

    // Ensure config directory exists
    let config_dir = PathBuf::from("config");
    ensure_dir_exists(&config_dir)?;
    
    // Write manager config
    let manager_config_path = config_dir.join("manager.json");
    if !manager_config_path.exists() {
        println!("Creating manager configuration: {:?}", manager_config_path);
        fs::write(&manager_config_path, manager_config)
            .context(format!("Failed to write manager config to {:?}", manager_config_path))?;
    }
    
    // Write crawler config
    let crawler_config_path = config_dir.join("crawler.json");
    if !crawler_config_path.exists() {
        println!("Creating crawler configuration: {:?}", crawler_config_path);
        fs::write(&crawler_config_path, crawler_config)
            .context(format!("Failed to write crawler config to {:?}", crawler_config_path))?;
    }
    
    println!("Created default configuration files");
    Ok(())
}

fn main() -> Result<()> {
    println!("Setting up CryptoCrawl project...");
    
    // Create default directories
    create_default_directories()?;
    
    // Create default configuration files
    create_default_configs()?;
    
    println!("Setup complete! You can now run the manager and crawler applications.");
    println!("To run the manager: cargo run --bin manager -- --config config/manager.json server");
    println!("To run the crawler: cargo run --bin crawler -- --config config/crawler.json");
    
    Ok(())
} 