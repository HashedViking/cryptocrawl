use std::process::Command;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    let package = args.get(1).unwrap_or(&"anyhow".to_string()).to_string();
    
    println!("Attempting to run daipendency CLI for: {}", package);
    
    // Run the daipendency CLI command
    let output = Command::new("daipendency")
        .args(["extract-dep", &package])
        .output();
    
    match output {
        Ok(output) => {
            println!("Exit status: {}", output.status);
            
            if output.status.success() {
                println!("Success! Output:");
                println!("{}", String::from_utf8_lossy(&output.stdout));
            } else {
                println!("Command failed. Error output:");
                println!("{}", String::from_utf8_lossy(&output.stderr));
            }
        },
        Err(e) => {
            println!("Failed to execute command: {}", e);
            println!("The daipendency CLI might not be installed. Try installing it with:");
            println!("cargo install daipendency --version 1.2.5");
        }
    }
} 