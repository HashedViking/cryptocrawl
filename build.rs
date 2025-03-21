use std::process::Command;
use std::env;
use std::path::Path;

fn main() {
    // Print build information
    println!("Building CryptoCrawl project...");
    
    // Get target directory
    let out_dir = env::var("OUT_DIR").unwrap_or_else(|_| "target".to_string());
    println!("Output directory: {}", out_dir);
    
    // Create directory structure
    for dir in ["data", "data/manager", "data/crawler", "keys", "logs", "config"] {
        let path = Path::new(&out_dir).join(dir);
        if !path.exists() {
            println!("Creating directory: {:?}", path);
            std::fs::create_dir_all(&path).expect(&format!("Failed to create directory: {:?}", path));
        }
    }
    
    // Build manager and crawler
    let cargo = if cfg!(windows) {
        "cargo.exe"
    } else {
        "cargo"
    };
    
    // Build manager
    println!("Building manager...");
    let status = Command::new(cargo)
        .args(&["build", "--manifest-path", "manager/Cargo.toml"])
        .status()
        .expect("Failed to build manager");
    
    if !status.success() {
        panic!("Manager build failed");
    }
    
    // Build crawler
    println!("Building crawler...");
    let status = Command::new(cargo)
        .args(&["build", "--manifest-path", "crawler/Cargo.toml"])
        .status()
        .expect("Failed to build crawler");
    
    if !status.success() {
        panic!("Crawler build failed");
    }
    
    println!("Build completed successfully!");
} 