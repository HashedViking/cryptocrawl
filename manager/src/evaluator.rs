use anyhow::{Result, anyhow};
use log::info;
use ollama_rs::Ollama;
use ollama_rs::generation::completion::{request::GenerationRequest, response::GenerationResponse};
use crate::models::{CrawlReport, CrawledPage};
use daipendency::DependencyExtractor;

/// Evaluator for crawl reports using LLM
pub struct Evaluator {
    /// Ollama client
    ollama: Ollama,
    /// Model to use
    model: String,
}

impl Evaluator {
    /// Create a new evaluator
    pub fn new(host: &str, model: &str) -> Self {
        let ollama = Ollama::new(host.to_string(), None);
        
        Evaluator {
            ollama,
            model: model.to_string(),
        }
    }
    
    /// Verify a crawl report using LLM
    pub async fn verify_report(&self, report: &CrawlReport) -> Result<(bool, f64, String)> {
        info!("Evaluating crawl report for task {} with {} pages", report.task_id, report.pages_count);
        
        // Extract relevant information for the LLM to evaluate
        let verification_prompt = self.create_verification_prompt(report);
        
        // Generate request
        let gen_req = GenerationRequest::new(
            self.model.clone(),
            verification_prompt
        );
        
        // Get response from Ollama
        let response = self.ollama.generate(gen_req).await?;
        
        // Parse the response - we expect a format like "SCORE: X.X\nVERIFIED: true/false\nREASONING: ..."
        let verification_result = self.parse_verification_response(&response)?;
        
        Ok(verification_result)
    }
    
    /// Create a prompt for the LLM to verify a crawl report
    fn create_verification_prompt(&self, report: &CrawlReport) -> String {
        let mut prompt = format!(
            "You are an expert web crawling validator. You need to verify if the following crawl report for the domain '{}' is valid and complete.\n\n",
            report.domain
        );
        
        prompt.push_str("CRAWL REPORT:\n");
        prompt.push_str(&format!("Domain: {}\n", report.domain));
        prompt.push_str(&format!("Pages crawled: {}\n", report.pages_count));
        prompt.push_str(&format!("Total data size: {} bytes\n", report.total_size));
        
        // Add details about the first few pages (limit to 5 to avoid too large prompts)
        prompt.push_str("\nSAMPLE PAGES:\n");
        for (i, page) in report.pages.iter().take(5).enumerate() {
            prompt.push_str(&format!("{}. URL: {}\n   Size: {} bytes\n", 
                i + 1, page.url, page.size));
        }
        
        prompt.push_str("\nEVALUATION TASK:\n");
        prompt.push_str("1. Assess if the crawl appears to be genuine and comprehensive for the given domain.\n");
        prompt.push_str("2. Check if the number of pages and data size are reasonable for the domain.\n");
        prompt.push_str("3. Verify if the URLs in the sample follow a pattern consistent with the domain.\n");
        prompt.push_str("4. Look for any anomalies or signs of gaming the system.\n\n");
        
        prompt.push_str("PROVIDE YOUR EVALUATION IN THIS EXACT FORMAT:\n");
        prompt.push_str("SCORE: [a number between 0.0 and 1.0 indicating confidence in validity]\n");
        prompt.push_str("VERIFIED: [true or false]\n");
        prompt.push_str("REASONING: [your detailed reasoning for the decision]\n");
        
        prompt
    }
    
    /// Parse the LLM's response to extract verification details
    fn parse_verification_response(&self, response: &GenerationResponse) -> Result<(bool, f64, String)> {
        let response_text = &response.response;
        
        // Extract score
        let score = if let Some(score_line) = response_text.lines()
            .find(|line| line.trim().starts_with("SCORE:")) {
            let score_part = score_line.trim().replace("SCORE:", "").trim().to_string();
            score_part.parse::<f64>().unwrap_or(0.0)
        } else {
            return Err(anyhow!("Could not parse score from LLM response"));
        };
        
        // Extract verification status
        let verified = if let Some(verified_line) = response_text.lines()
            .find(|line| line.trim().starts_with("VERIFIED:")) {
            let verified_part = verified_line.trim().replace("VERIFIED:", "").trim().to_string();
            verified_part.to_lowercase() == "true"
        } else {
            return Err(anyhow!("Could not parse verification status from LLM response"));
        };
        
        // Extract reasoning
        let reasoning = if let Some(reasoning_index) = response_text.find("REASONING:") {
            response_text[reasoning_index..].replace("REASONING:", "").trim().to_string()
        } else {
            "No detailed reasoning provided".to_string()
        };
        
        Ok((verified, score, reasoning))
    }
    
    /// Get API documentation using daipendency
    pub async fn get_api_documentation(&self, package_name: &str) -> Result<String> {
        let extractor = DependencyExtractor::new();
        
        // Extract documentation
        let docs = extractor.extract_docs(package_name).await?;
        
        // Format as markdown
        let mut formatted_docs = format!("# API Documentation for {}\n\n", package_name);
        
        // Add classes, methods, etc.
        for item in docs.items {
            formatted_docs.push_str(&format!("## {}\n\n", item.name));
            
            if let Some(description) = item.description {
                formatted_docs.push_str(&format!("{}\n\n", description));
            }
            
            if !item.methods.is_empty() {
                formatted_docs.push_str("### Methods\n\n");
                
                for method in item.methods {
                    formatted_docs.push_str(&format!("#### {}\n\n", method.name));
                    
                    if let Some(description) = method.description {
                        formatted_docs.push_str(&format!("{}\n\n", description));
                    }
                    
                    if !method.params.is_empty() {
                        formatted_docs.push_str("Parameters:\n");
                        
                        for param in method.params {
                            formatted_docs.push_str(&format!("- `{}`: {}\n", 
                                param.name,
                                param.description.unwrap_or_else(|| "No description".to_string())));
                        }
                        
                        formatted_docs.push_str("\n");
                    }
                    
                    if let Some(returns) = method.returns {
                        formatted_docs.push_str(&format!("Returns: {}\n\n", returns));
                    }
                }
            }
            
            if !item.properties.is_empty() {
                formatted_docs.push_str("### Properties\n\n");
                
                for prop in item.properties {
                    formatted_docs.push_str(&format!("- `{}`: {}\n", 
                        prop.name,
                        prop.description.unwrap_or_else(|| "No description".to_string())));
                }
                
                formatted_docs.push_str("\n");
            }
        }
        
        Ok(formatted_docs)
    }
} 