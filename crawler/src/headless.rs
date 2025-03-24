use anyhow::{Result, anyhow};
use chromiumoxide::{Browser, BrowserConfig, Element, Page};
use futures::StreamExt;
use log::{info, warn, debug, error};
use std::time::Duration;
use tokio::time::timeout;
use url::Url;
use std::sync::Arc;
use std::collections::HashSet;
use std::process::Command;

/// HeadlessBrowser provides browser automation for JavaScript-heavy sites
#[derive(Clone)]
pub struct HeadlessBrowser {
    /// The browser instance
    browser: Option<Arc<Browser>>,
    /// Whether the browser is currently running
    is_running: bool,
}

impl Default for HeadlessBrowser {
    fn default() -> Self {
        Self {
            browser: None,
            is_running: false,
        }
    }
}

impl HeadlessBrowser {
    /// Create a new headless browser
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Start the browser
    pub async fn start(&mut self) -> Result<()> {
        if self.is_running {
            return Ok(());
        }
        
        info!("Starting headless Chrome browser");
        
        // Create browser config with more robust settings
        let config = BrowserConfig::builder()
            .no_sandbox() // Often needed in Docker or CI environments
            .incognito() // Use incognito mode to avoid cache/cookies between sessions
            .args(vec![
                "--disable-web-security", // Disable CORS for easier crawling
                "--disable-extensions",   // No extensions needed
                "--disable-gpu",          // Better compatibility
                "--disable-dev-shm-usage", // Avoid crashes in constrained environments
                "--disable-setuid-sandbox", // Additional sandbox flexibility
                "--no-first-run",         // Skip first run tasks
                "--no-zygote"             // More robust launching
            ])
            .build()
            .map_err(|e| anyhow!("Failed to build browser config: {}", e))?;
        
        // Try to launch the browser with retries
        let mut retries = 3;
        let mut last_error = None;
        
        while retries > 0 {
            match chromiumoxide::Browser::launch(config.clone()).await {
                Ok((browser, mut handler)) => {
                    // Spawn a task to handle browser events
                    tokio::spawn(async move {
                        while let Some(h) = handler.next().await {
                            if let Err(e) = h {
                                warn!("Browser handler error: {}", e);
                            }
                        }
                    });
                    
                    self.browser = Some(Arc::new(browser));
                    self.is_running = true;
                    
                    info!("Headless Chrome browser started successfully");
                    return Ok(());
                },
                Err(e) => {
                    error!("Failed to launch browser (attempt {}): {}", 4 - retries, e);
                    last_error = Some(e);
                    retries -= 1;
                    
                    if retries > 0 {
                        // Check if Chrome processes need to be killed
                        if cfg!(windows) {
                            let _ = Command::new("taskkill")
                                .args(&["/F", "/IM", "chrome.exe"])
                                .output();
                            let _ = Command::new("taskkill")
                                .args(&["/F", "/IM", "chromedriver.exe"])
                                .output();
                        }
                        
                        // Wait before retrying
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        }
        
        // If we get here, all retries have failed
        Err(anyhow!("Failed to start Chrome browser after multiple attempts: {:?}", last_error))
    }
    
    /// Stop the browser
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(browser) = self.browser.take() {
            info!("Stopping headless Chrome browser");
            
            // We can't call close() directly on Arc<Browser>
            // Just drop the reference and let the browser be cleaned up
            drop(browser);
            
            self.is_running = false;
            info!("Headless Chrome browser stopped");
            
            // Clean up any stray processes
            if cfg!(windows) {
                let _ = Command::new("taskkill")
                    .args(&["/F", "/IM", "chrome.exe"])
                    .output();
            }
        }
        
        Ok(())
    }
    
    /// Extract links from a JavaScript-heavy page
    pub async fn extract_links(&self, url: &Url, wait_time_secs: u64) -> Result<Vec<Url>> {
        let browser = self.browser.as_ref()
            .ok_or_else(|| anyhow!("Browser not started"))?;
            
        info!("HeadlessBrowser::extract_links called for {}", url);
        
        // Use a shorter overall timeout
        let total_timeout = timeout(Duration::from_secs(wait_time_secs + 5), async {
            // Create a new page with error handling
            let page = match browser.new_page(url.as_str()).await {
                Ok(page) => page,
                Err(e) => return Err(anyhow!("Failed to create new page: {}", e)),
            };
            
            // Set a reasonable timeout for navigation
            let timeout_duration = Duration::from_secs(wait_time_secs.max(3));
            
            debug!("Waiting for page to load...");
            
            // Wait for navigation to complete with timeout
            let wait_for_result = timeout(
                timeout_duration,
                page.wait_for_navigation()
            ).await;
            
            if let Err(_) = wait_for_result {
                warn!("Timeout waiting for page navigation, will try to extract content anyway");
            }
            
            // Quickly scroll to try to trigger lazy-loading
            if let Err(e) = page.evaluate("window.scrollTo(0, document.body.scrollHeight * 0.3);").await {
                debug!("Failed to scroll: {}", e);
            }
            
            // Extract all links from the page
            let links = match self.extract_links_from_page(&page).await {
                Ok(links) => links,
                Err(e) => {
                    // Try to close the page to prevent leaks
                    let _ = page.close().await;
                    return Err(anyhow!("Failed to extract links: {}", e));
                }
            };
            
            // Close the page immediately to free resources
            if let Err(e) = page.close().await {
                warn!("Error closing page: {}", e);
            }
            
            Ok(links)
        }).await;
        
        match total_timeout {
            Ok(result) => result,
            Err(_) => {
                warn!("Overall timeout extracting links from {}", url);
                Err(anyhow!("Timeout while extracting links"))
            }
        }
    }
    
    /// Extract content from a JavaScript-heavy page
    pub async fn extract_content(&self, url: &Url, wait_time_secs: u64) -> Result<String> {
        let browser = self.browser.as_ref()
            .ok_or_else(|| anyhow!("Browser not started"))?;
            
        info!("Navigating to {} to extract content", url);
        
        // Use a shorter overall timeout
        let total_timeout = timeout(Duration::from_secs(wait_time_secs + 5), async {
            // Create a new page with error handling
            let page = match browser.new_page(url.as_str()).await {
                Ok(page) => page,
                Err(e) => return Err(anyhow!("Failed to create new page: {}", e)),
            };
            
            // Set a reasonable timeout for navigation
            let timeout_duration = Duration::from_secs(wait_time_secs.max(3));
            
            debug!("Waiting for page to load...");
            
            // Wait for navigation to complete with timeout
            let wait_for_result = timeout(
                timeout_duration,
                page.wait_for_navigation()
            ).await;
            
            if let Err(_) = wait_for_result {
                warn!("Timeout waiting for page navigation, will try to extract content anyway");
            }
            
            // Fix for infinite loading pages: always wait for a short time
            tokio::time::sleep(Duration::from_millis(500)).await;
            
            // Fast scroll to trigger lazy-loading
            let scroll_result = page.evaluate("
                window.scrollTo(0, document.body.scrollHeight * 0.3); 
                setTimeout(() => window.scrollTo(0, document.body.scrollHeight * 0.7), 200);
            ").await;
            
            if let Err(e) = scroll_result {
                debug!("Failed to scroll: {}", e);
            }
            
            // Get the page content immediately after scrolling
            let content = match timeout(
                Duration::from_secs(2), 
                page.content()
            ).await {
                Ok(Ok(content)) => content,
                Ok(Err(e)) => {
                    // Try to close the page
                    let _ = page.close().await;
                    return Err(anyhow!("Failed to get page content: {}", e));
                },
                Err(_) => {
                    // Try to close the page
                    let _ = page.close().await;
                    return Err(anyhow!("Timeout getting page content"));
                },
            };
            
            // Close the page immediately to free resources
            if let Err(e) = page.close().await {
                warn!("Error closing page: {}", e);
            }
            
            Ok(content)
        }).await;
        
        match total_timeout {
            Ok(result) => result,
            Err(_) => {
                warn!("Overall timeout extracting content from {}", url);
                Err(anyhow!("Timeout while extracting content"))
            }
        }
    }
    
    /// Extract links from a page
    async fn extract_links_from_page(&self, page: &Page) -> Result<Vec<Url>> {
        let base_url_str = page.url().await
            .map_err(|e| anyhow!("Failed to get page URL: {}", e))?
            .ok_or_else(|| anyhow!("Page URL is None"))?;
        
        let base_url = Url::parse(&base_url_str)
            .map_err(|e| anyhow!("Failed to parse page URL '{}': {}", base_url_str, e))?;
        
        // Execute JavaScript to get all links
        let elements: Vec<Element> = timeout(
            Duration::from_secs(10),
            page.find_elements("a")
        ).await
        .map_err(|_| anyhow!("Timeout getting links"))?
        .map_err(|e| anyhow!("Failed to find links: {}", e))?;
        
        info!("Found {} potential link elements", elements.len());
        
        let mut links = HashSet::new();
        
        // Process each link
        for element in elements {
            if let Ok(Some(href)) = element.attribute("href").await {
                debug!("Found link: {}", href);
                
                // Try to parse the href as a URL
                match base_url.join(&href) {
                    Ok(url) => {
                        // Only keep http/https URLs
                        if url.scheme() == "http" || url.scheme() == "https" {
                            // Remove fragment
                            let mut clean_url = url.clone();
                            clean_url.set_fragment(None);
                            
                            links.insert(clean_url);
                        }
                    }
                    Err(e) => {
                        debug!("Failed to parse URL '{}': {}", href, e);
                    }
                }
            }
        }
        
        let result: Vec<Url> = links.into_iter().collect();
        info!("Extracted {} unique links from page", result.len());
        
        Ok(result)
    }
    
    /// Take a screenshot of a page (useful for debugging)
    #[allow(dead_code)]
    pub async fn take_screenshot(&self, url: &Url, path: &str) -> Result<()> {
        let browser = self.browser.as_ref()
            .ok_or_else(|| anyhow!("Browser not started"))?;
            
        info!("Taking screenshot of {}", url);
        
        // Create a new page
        let page = browser.new_page(url.as_str()).await
            .map_err(|e| anyhow!("Failed to create new page: {}", e))?;
        
        // Wait for page to load
        tokio::time::sleep(Duration::from_secs(5)).await;
        
        // Take screenshot using the correct API
        use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotParams;
        let params = CaptureScreenshotParams::default();
        let screenshot_data = page.screenshot(params).await
            .map_err(|e| anyhow!("Failed to take screenshot: {}", e))?;
        
        // Save to file
        tokio::fs::write(path, screenshot_data)
            .await
            .map_err(|e| anyhow!("Failed to save screenshot: {}", e))?;
        
        // Close the page
        if let Err(e) = page.close().await {
            warn!("Error closing page: {}", e);
        }
        
        info!("Screenshot saved to {}", path);
        Ok(())
    }
}

impl Drop for HeadlessBrowser {
    fn drop(&mut self) {
        if self.is_running {
            info!("HeadlessBrowser being dropped, browser will be closed");
            // We can't do async operations in Drop, so we just log a warning
        }
    }
} 