use anyhow::{Result, anyhow, Context};
use log::{info, warn, debug};
use crate::models::CrawlReport;
use reqwest::Client;
use std::time::Duration;
use std::process::Command;
use std::fs;

/// LLM-based evaluator for crawl reports
pub struct Evaluator {
    /// Ollama host URL
    host: String,
    /// Ollama model to use
    model: String,
    /// HTTP client
    client: Client,
}

impl Evaluator {
    /// Create a new evaluator instance
    pub fn new(host: &str, model: &str) -> Self {
        Self {
            host: host.to_string(),
            model: model.to_string(),
            client: Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }
    
    /// Verify a crawl report using LLM
    pub async fn verify_report(&self, report: &CrawlReport) -> Result<(bool, f64, String)> {
        // Create verification prompt
        let prompt = self.create_verification_prompt(report);
        
        // Query LLM
        info!("Querying LLM to verify report with {} pages", report.pages_count);
        let response = self.query_llm(&prompt).await?;
        
        // Extract verification result
        let (is_valid, confidence, reason) = self.parse_verification_result(&response)?;
        
        info!("Report verification result: valid={}, confidence={:.2}, reason={}",
              is_valid, confidence, reason);
        
        Ok((is_valid, confidence, reason))
    }
    
    /// Get API documentation for a package using daipendency
    pub async fn get_api_documentation(&self, package: &str) -> Result<String> {
        info!("Extracting API documentation for package: {}", package);
        
        // Use the daipendency CLI to extract API documentation
        let output = Command::new("daipendency")
            .args(["extract-dep", package, "--language=rust"])
            .output()
            .context("Failed to run daipendency CLI")?;
        
        if !output.status.success() {
            return Err(anyhow!("daipendency CLI failed: {}", String::from_utf8_lossy(&output.stderr)));
        }
        
        let docs = String::from_utf8_lossy(&output.stdout).to_string();
        debug!("Extracted API documentation for {}", package);
        
        // Cache the documentation for future use
        let cache_dir = "cache/api_docs";
        if !std::path::Path::new(cache_dir).exists() {
            fs::create_dir_all(cache_dir)?;
        }
        
        let cache_path = format!("{}/{}.md", cache_dir, package);
        fs::write(&cache_path, &docs)
            .context(format!("Failed to cache API documentation for {} to {}", package, cache_path))?;
        
        info!("API documentation for {} saved to {}", package, cache_path);
        
        // Enhance documentation with LLM insights
        if let Ok(enhanced_docs) = self.enhance_documentation_with_llm(&docs, package).await {
            return Ok(enhanced_docs);
        }
        
        Ok(docs)
    }
    
    /// Enhance API documentation with LLM insights and examples
    async fn enhance_documentation_with_llm(&self, docs: &str, package: &str) -> Result<String> {
        let prompt = format!(
            "You are an expert Rust developer. Below is the API documentation for the {} crate:
            
            {}
            
            Please enhance this documentation by:
            1. Adding usage examples for the most important functions/methods
            2. Explaining common patterns and best practices
            3. Identifying potential pitfalls or gotchas
            4. Providing context on how different components relate to each other
            
            Format your response as Markdown, preserving the original documentation and adding your enhancements.",
            package, docs
        );
        
        match self.query_llm(&prompt).await {
            Ok(response) => {
                info!("Enhanced API documentation for {} with LLM insights", package);
                Ok(response)
            },
            Err(e) => {
                warn!("Failed to enhance documentation with LLM: {}", e);
                Ok(docs.to_string()) // Return original docs if enhancement fails
            }
        }
    }
    
    /// Create verification prompt for LLM
    fn create_verification_prompt(&self, report: &CrawlReport) -> String {
        // Calculate crawl duration in ms
        let duration = match report.end_time {
            Some(end) => (end - report.start_time) * 1000, // Convert seconds to ms
            None => 0,
        };
        
        let mut prompt = format!(
            "You are a web crawl verification agent. Please verify the following crawl report:
            
            Task ID: {}
            Pages Crawled: {}
            Total Size: {} bytes
            Crawl Duration: {} ms
            
            Please analyze the crawled pages and verify:
            1. That the page sizes look reasonable
            2. That the content types are valid
            3. That the URL structure is consistent
            4. That there are no obvious fake or malicious entries
            
            The first 10 crawled pages are:
            ",
            report.task_id,
            report.pages_count,
            report.total_size,
            duration
        );
        
        // Add up to 10 page samples
        for (i, page) in report.pages.iter().take(10).enumerate() {
            prompt.push_str(&format!(
                "{}. URL: {}, Size: {} bytes, Content-Type: {}, Status: {}\n",
                i + 1,
                page.url,
                page.size,
                page.content_type.as_deref().unwrap_or("unknown"),
                page.status.unwrap_or(0)
            ));
        }
        
        prompt.push_str("\nBased on the above information, please respond with:
            
        VALID: [true/false]
        CONFIDENCE: [0.0-1.0]
        REASON: [brief explanation of your decision]");
        
        prompt
    }
    
    /// Query Ollama LLM
    async fn query_llm(&self, prompt: &str) -> Result<String> {
        let url = format!("{}/api/generate", self.host);
        
        let response = self.client.post(&url)
            .json(&serde_json::json!({
                "model": self.model,
                "prompt": prompt,
                "stream": false
            }))
            .send()
            .await
            .context("Failed to query LLM")?;
        
        if response.status().is_success() {
            let result: serde_json::Value = response.json().await
                .context("Failed to parse LLM response")?;
            
            if let Some(response_text) = result.get("response").and_then(|v| v.as_str()) {
                Ok(response_text.to_string())
            } else {
                Err(anyhow::anyhow!("Invalid LLM response format"))
            }
        } else {
            let status = response.status();
            let error_text = response.text().await
                .unwrap_or_else(|_| "Unknown error".to_string());
            
            Err(anyhow::anyhow!("LLM API error: {} - {}", status, error_text))
        }
    }
    
    /// Parse verification result from LLM response
    fn parse_verification_result(&self, response: &str) -> Result<(bool, f64, String)> {
        // Extract valid flag
        let valid_line = response.lines()
            .find(|line| line.trim().starts_with("VALID:"))
            .unwrap_or("VALID: false");
        
        let valid = valid_line.contains("true");
        
        // Extract confidence
        let confidence_line = response.lines()
            .find(|line| line.trim().starts_with("CONFIDENCE:"))
            .unwrap_or("CONFIDENCE: 0.0");
        
        let confidence: f64 = confidence_line
            .split(':')
            .nth(1)
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0.0);
        
        // Extract reason
        let reason_line = response.lines()
            .find(|line| line.trim().starts_with("REASON:"))
            .unwrap_or("REASON: Unknown");
        
        let reason = reason_line
            .split(':')
            .nth(1)
            .unwrap_or("Unknown")
            .trim()
            .to_string();
        
        Ok((valid, confidence, reason))
    }
} 