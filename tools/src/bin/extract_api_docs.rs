use anyhow::{Result, Context};
use std::fs;
use std::path::Path;
use std::env;
use std::process::Command;

/// Extract API documentation for a package and save it to a file using the daipendency CLI
async fn extract_api_docs(package: &str, output_file: Option<&str>) -> Result<()> {
    println!("Extracting API documentation for {} ...", package);
    
    // Use the daipendency CLI to extract API documentation
    let output = Command::new("daipendency")
        .args(["extract-dep", package, "--language=rust"])
        .output()
        .context("Failed to run daipendency CLI")?;
    
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "daipendency CLI failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    
    let result_str = String::from_utf8_lossy(&output.stdout).to_string();
    println!("Extracted API documentation successfully");
    
    // Save to file or print to console
    match output_file {
        Some(file_path) => {
            // Create directory if it doesn't exist
            if let Some(parent) = Path::new(file_path).parent() {
                if !parent.exists() {
                    fs::create_dir_all(parent)
                        .context(format!("Failed to create directory for {}", file_path))?;
                }
            }
            
            // Write to file
            fs::write(file_path, &result_str)
                .context(format!("Failed to write documentation to {}", file_path))?;
            
            println!("API documentation saved to {}", file_path);
        },
        None => {
            // Print to console
            println!("\n{}", result_str);
        }
    }
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Get command-line arguments
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        println!("Usage: {} PACKAGE_NAME [OUTPUT_FILE]", args[0]);
        println!("Example: {} spider docs/spider-api.md", args[0]);
        return Ok(());
    }
    
    let package = &args[1];
    let output_file = args.get(2).map(|s| s.as_str());
    
    // Extract API documentation
    extract_api_docs(package, output_file).await?;
    
    Ok(())
} 