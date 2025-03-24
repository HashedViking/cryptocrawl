use anyhow::{Result, anyhow};
use url::Url;
use log::{info, warn, debug};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, SystemTime};
use reqwest::Client;
use std::sync::Arc;
use std::sync::Mutex;

/// Simple robots.txt parser
#[derive(Default, Clone, Debug)]
pub struct RobotsTxt {
    /// Rules for specific user agents
    rules: HashMap<String, Vec<Rule>>,
    /// Rules for all user agents (*)
    default_rules: Vec<Rule>,
}

/// Rule type for robots.txt
#[derive(Clone, Debug)]
enum Rule {
    /// Allow a path
    Allow(String),
    /// Disallow a path
    Disallow(String),
}

impl RobotsTxt {
    /// Parse robots.txt content
    pub fn parse(&mut self, content: &str) {
        self.rules.clear();
        self.default_rules.clear();
        
        let mut current_agents: Vec<String> = Vec::new();
        
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            
            // Split at the first colon
            if let Some(idx) = line.find(':') {
                let (directive, value) = line.split_at(idx);
                let directive = directive.trim().to_lowercase();
                let value = value[1..].trim(); // Skip the colon
                
                match directive.as_str() {
                    "user-agent" => {
                        // If we were parsing rules and suddenly hit a new user-agent,
                        // start a new user-agent section
                        if !current_agents.is_empty() && self.rules.get(current_agents[0].as_str()).is_some() {
                            current_agents.clear();
                        }
                        
                        current_agents.push(value.to_lowercase());
                    },
                    "allow" => {
                        let rule = Rule::Allow(value.to_string());
                        if current_agents.is_empty() {
                            // No user agent context, add to default rules
                            self.default_rules.push(rule);
                        } else {
                            // Add to each current user agent
                            for agent in &current_agents {
                                self.rules.entry(agent.clone())
                                    .or_insert_with(Vec::new)
                                    .push(rule.clone());
                            }
                        }
                    },
                    "disallow" => {
                        if value.is_empty() {
                            // Empty disallow = allow all
                            continue;
                        }
                        
                        let rule = Rule::Disallow(value.to_string());
                        if current_agents.is_empty() {
                            // No user agent context, add to default rules
                            self.default_rules.push(rule);
                        } else {
                            // Add to each current user agent
                            for agent in &current_agents {
                                self.rules.entry(agent.clone())
                                    .or_insert_with(Vec::new)
                                    .push(rule.clone());
                            }
                        }
                    },
                    // We ignore other directives like sitemap, crawl-delay, etc.
                    // We'll handle sitemaps separately
                    _ => {}
                }
            }
        }
    }
    
    /// Check if user agent is allowed to fetch URL
    pub fn can_fetch(&self, user_agent: &str, url: &Url) -> bool {
        let path = url.path();
        
        // First check specific rules for this user agent
        let user_agent = user_agent.to_lowercase();
        if let Some(rules) = self.rules.get(&user_agent) {
            if let Some(allowed) = self.check_rules(rules, path) {
                return allowed;
            }
        }
        
        // Then check specific rules that might apply to a prefix of the user agent
        for (agent, rules) in &self.rules {
            if user_agent.starts_with(agent) {
                if let Some(allowed) = self.check_rules(rules, path) {
                    return allowed;
                }
            }
        }
        
        // Finally check the default rules (* user agent)
        if let Some(rules) = self.rules.get("*") {
            if let Some(allowed) = self.check_rules(rules, path) {
                return allowed;
            }
        }
        
        // If no specific rules matched, use default rules
        if let Some(allowed) = self.check_rules(&self.default_rules, path) {
            return allowed;
        }
        
        // By default, allow if no rules matched
        true
    }
    
    /// Check if path matches any rules
    fn check_rules(&self, rules: &[Rule], path: &str) -> Option<bool> {
        let mut matched = false;
        let mut result = false;
        
        for rule in rules {
            match rule {
                Rule::Allow(pattern) => {
                    if self.path_matches(pattern, path) {
                        matched = true;
                        result = true;
                    }
                },
                Rule::Disallow(pattern) => {
                    if self.path_matches(pattern, path) {
                        matched = true;
                        result = false;
                    }
                },
            }
        }
        
        if matched {
            Some(result)
        } else {
            None
        }
    }
    
    /// Check if path matches pattern
    fn path_matches(&self, pattern: &str, path: &str) -> bool {
        if pattern == "/" {
            // Special case: pattern "/" matches everything
            return true;
        }
        
        if pattern.ends_with('*') {
            // Handle wildcard at the end
            let prefix = &pattern[..pattern.len() - 1];
            path.starts_with(prefix)
        } else {
            // Otherwise exact match or directory prefix
            path == pattern || path.starts_with(&format!("{}/", pattern))
        }
    }
}

/// Extracts URLs from XML content using simple string search
/// This avoids using scraper which is not Send-compatible
fn extract_urls_from_sitemap(content: &str) -> (Vec<String>, Vec<String>) {
    let mut sitemap_urls = Vec::new();
    let mut page_urls = Vec::new();
    
    // Look for sitemap URLs (in sitemap index)
    let mut pos = 0;
    while let Some(loc_start) = content[pos..].find("<loc>") {
        pos += loc_start + 5; // 5 is the length of "<loc>"
        if let Some(loc_end) = content[pos..].find("</loc>") {
            let url = content[pos..pos + loc_end].trim();
            
            // Determine if this is a sitemap URL or a page URL
            // by checking if it's inside a <sitemap> tag
            let preceding = &content[..pos];
            let last_open_tag = preceding.rfind('<').and_then(|idx| {
                let tag_start = &preceding[idx..];
                if tag_start.starts_with("<sitemap") || tag_start.starts_with("<sitemap>") {
                    Some("sitemap")
                } else if tag_start.starts_with("<url") || tag_start.starts_with("<url>") {
                    Some("url")
                } else {
                    None
                }
            });
            
            if let Some(tag_type) = last_open_tag {
                if tag_type == "sitemap" {
                    sitemap_urls.push(url.to_string());
                } else {
                    page_urls.push(url.to_string());
                }
            } else {
                // If we can't determine, assume it's a page URL
                page_urls.push(url.to_string());
            }
            
            pos += loc_end + 6; // 6 is the length of "</loc>"
        } else {
            break;
        }
    }
    
    (sitemap_urls, page_urls)
}

/// Manager for robots.txt handling and JavaScript detection
#[derive(Debug, Clone)]
pub struct RobotsManager {
    /// Cache of robots.txt parsers by domain
    robots_cache: HashMap<String, (RobotsTxt, SystemTime)>,
    /// Cache of sitemaps by domain
    sitemap_cache: HashMap<String, (HashSet<String>, SystemTime)>,
    /// User agent to use for robots.txt
    user_agent: String,
    /// Cache validity duration
    cache_validity: Duration,
    /// HTTP client for fetching robots.txt and sitemaps
    client: Client,
    /// Negative cache - domains that don't have robots.txt
    negative_cache: HashSet<String>,
    /// Thread-local cache of allowed URLs - changed to Mutex for thread safety
    allowed_urls_cache: Option<Arc<Mutex<VecDeque<(String, bool, SystemTime)>>>>,
}

impl Default for RobotsManager {
    fn default() -> Self {
        Self {
            robots_cache: HashMap::new(),
            sitemap_cache: HashMap::new(),
            user_agent: "CryptoCrawl/0.1 (https://github.com/yourusername/cryptocrawl)".to_string(),
            cache_validity: Duration::from_secs(3600), // 1 hour
            client: Client::new(),
            negative_cache: HashSet::new(),
            allowed_urls_cache: Some(Arc::new(Mutex::new(VecDeque::with_capacity(100)))),
        }
    }
}

impl RobotsManager {
    /// Create a new robots manager with custom user agent
    pub fn new(user_agent: &str) -> Self {
        let mut manager = Self::default();
        manager.user_agent = user_agent.to_string();
        manager
    }
    
    /// Set the cache validity duration
    pub fn with_cache_validity(mut self, duration: Duration) -> Self {
        self.cache_validity = duration;
        self
    }
    
    /// Set the HTTP client
    pub fn with_client(mut self, client: Client) -> Self {
        self.client = client;
        self
    }
    
    /// Check if a URL is allowed to be crawled
    pub async fn is_allowed(&mut self, url: &Url) -> Result<bool> {
        let url_str = url.to_string();
        
        // Check thread-local cache first
        if let Some(ref cache) = self.allowed_urls_cache {
            // Check if we have a cached result
            let cache_hit = {
                let mut cache_guard = cache.lock().unwrap();
                
                // Look for cached result
                let now = SystemTime::now();
                let cache_entry = cache_guard.iter().find(|(u, _, timestamp)| {
                    u == &url_str && now.duration_since(*timestamp).unwrap_or_default() <= Duration::from_secs(60)
                });
                
                // If found, return the cached result
                if let Some((_, allowed, _)) = cache_entry {
                    return Ok(*allowed);
                }
                
                // Prune old entries if cache is getting too large
                if cache_guard.len() > 1000 {
                    // Remove the oldest 20% of entries
                    let to_remove = cache_guard.len() / 5;
                    for _ in 0..to_remove {
                        cache_guard.pop_front();
                    }
                }
                
                false
            };
            
            // If we found a cache hit, we've already returned
            if cache_hit {
                return Ok(true); // This never executes, but keeps the compiler happy
            }
        }
        
        let domain = url.host_str()
            .ok_or_else(|| anyhow!("URL has no host"))?
            .to_string();
            
        // Check negative cache - domains we know don't have robots.txt
        if self.negative_cache.contains(&domain) {
            // Cache result
            if let Some(ref cache) = self.allowed_urls_cache {
                let mut cache_guard = cache.lock().unwrap();
                cache_guard.push_back((url_str, true, SystemTime::now()));
            }
            return Ok(true);
        }
            
        // Clone the user agent to avoid borrowing issues
        let user_agent = self.user_agent.clone();
            
        // Get or fetch robots.txt
        let robots = match self.get_robots_parser(&domain).await {
            Ok(robots) => robots,
            Err(e) => {
                // If we failed to get robots.txt, assume allowed and cache the domain
                debug!("Failed to get robots.txt for {}: {}", domain, e);
                self.negative_cache.insert(domain);
                
                // Cache result
                if let Some(ref cache) = self.allowed_urls_cache {
                    let mut cache_guard = cache.lock().unwrap();
                    cache_guard.push_back((url_str, true, SystemTime::now()));
                }
                
                return Ok(true);
            }
        };
        
        // Check if path is allowed
        let allowed = robots.can_fetch(&user_agent, url);
        
        // Cache result
        if let Some(ref cache) = self.allowed_urls_cache {
            let mut cache_guard = cache.lock().unwrap();
            cache_guard.push_back((url_str, allowed, SystemTime::now()));
        }
        
        Ok(allowed)
    }
    
    /// Get the robots.txt parser for a domain
    async fn get_robots_parser(&mut self, domain: &str) -> Result<&RobotsTxt> {
        // Check if we have a valid cached entry
        let needs_refresh = match self.robots_cache.get(domain) {
            Some((_, timestamp)) => {
                let now = SystemTime::now();
                now.duration_since(*timestamp)
                    .unwrap_or_default() > self.cache_validity
            }
            None => true,
        };
        
        // Fetch and parse robots.txt if needed
        if needs_refresh {
            info!("Fetching robots.txt for domain: {}", domain);
            let robots_url = format!("http://{}/robots.txt", domain);
            
            // Create a new parser
            let mut robots = RobotsTxt::default();
            
            // Try to read the robots.txt file
            match self.client.get(&robots_url).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        let robots_content = match response.text().await {
                            Ok(text) => text,
                            Err(e) => {
                                warn!("Failed to read robots.txt for {}: {}", domain, e);
                                String::new() // Empty robots.txt (all allowed)
                            }
                        };
                        
                        // Parse the robots.txt content
                        robots.parse(&robots_content);
                    } else {
                        debug!("No robots.txt found for {} (status: {})", domain, response.status());
                        // Default parser (all allowed)
                    }
                }
                Err(e) => {
                    warn!("Failed to fetch robots.txt for {}: {}", domain, e);
                    // Default parser (all allowed)
                }
            };
            
            // Store in cache
            self.robots_cache.insert(domain.to_string(), (robots, SystemTime::now()));
        }
        
        // Return the parser
        Ok(&self.robots_cache.get(domain)
            .expect("Parser should exist at this point")
            .0)
    }
    
    /// Extract sitemap URLs from robots.txt
    pub async fn get_sitemaps_from_robots(&mut self, domain: &str) -> Result<Vec<String>> {
        // Extract sitemaps
        let mut sitemaps = Vec::new();
        
        // Manually check for "Sitemap:" lines in robots.txt
        let robots_url = format!("http://{}/robots.txt", domain);
        match self.client.get(&robots_url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let robots_content = match response.text().await {
                        Ok(text) => text,
                        Err(e) => {
                            warn!("Failed to read robots.txt for {}: {}", domain, e);
                            String::new() // Empty robots.txt
                        }
                    };
                    
                    // Parse the robots.txt content for Sitemap: lines
                    for line in robots_content.lines() {
                        let line = line.trim().to_lowercase();
                        if line.starts_with("sitemap:") {
                            if let Some(url) = line.strip_prefix("sitemap:") {
                                let url = url.trim();
                                if !url.is_empty() {
                                    sitemaps.push(url.to_string());
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Failed to fetch robots.txt for {}: {}", domain, e);
            }
        }
        
        Ok(sitemaps)
    }
    
    /// Get all URLs from a domain's sitemaps
    pub async fn get_sitemap_urls(&mut self, domain: &str) -> Result<HashSet<String>> {
        // Check cache
        if let Some((urls, timestamp)) = self.sitemap_cache.get(domain) {
            let now = SystemTime::now();
            if now.duration_since(*timestamp).unwrap_or_default() <= self.cache_validity {
                return Ok(urls.clone());
            }
        }
        
        // Get sitemaps from robots.txt
        let sitemap_urls = self.get_sitemaps_from_robots(domain).await?;
        
        // If no sitemaps in robots.txt, try the standard location
        let sitemap_urls = if sitemap_urls.is_empty() {
            vec![format!("http://{}/sitemap.xml", domain)]
        } else {
            sitemap_urls
        };
        
        // Process each sitemap
        let mut all_urls = HashSet::new();
        let mut visited_sitemaps = HashSet::new();
        
        // Create an Arc<Client> to share across async tasks
        let client = Arc::new(self.client.clone());
        
        // Process initial sitemaps
        for sitemap_url in sitemap_urls {
            self.process_sitemap_non_recursive(
                &sitemap_url, 
                client.clone(), 
                &mut all_urls, 
                &mut visited_sitemaps
            ).await?;
        }
        
        // Cache the results
        self.sitemap_cache.insert(
            domain.to_string(), 
            (all_urls.clone(), SystemTime::now())
        );
        
        Ok(all_urls)
    }
    
    /// Process a sitemap without recursion
    /// Instead maintains a queue of sitemaps to process
    async fn process_sitemap_non_recursive(
        &self, 
        initial_sitemap_url: &str, 
        client: Arc<Client>,
        all_urls: &mut HashSet<String>,
        visited_sitemaps: &mut HashSet<String>
    ) -> Result<()> {
        // Stack of sitemaps to process
        let mut sitemap_stack = vec![initial_sitemap_url.to_string()];
        
        // Mark initial sitemap as visited
        visited_sitemaps.insert(initial_sitemap_url.to_string());
        
        while let Some(sitemap_url) = sitemap_stack.pop() {
            info!("Processing sitemap: {}", sitemap_url);
            
            // Fetch sitemap
            let response = match client.get(&sitemap_url).send().await {
                Ok(resp) => resp,
                Err(e) => {
                    warn!("Failed to fetch sitemap {}: {}", sitemap_url, e);
                    continue;
                }
            };
            
            if !response.status().is_success() {
                warn!("Failed to fetch sitemap {}: status {}", sitemap_url, response.status());
                continue;
            }
            
            // Get the sitemap content
            let content = match response.text().await {
                Ok(text) => text,
                Err(e) => {
                    warn!("Failed to get text from sitemap {}: {}", sitemap_url, e);
                    continue;
                }
            };
            
            // Extract URLs using a simpler method that doesn't use scraper
            let (sub_sitemaps, page_urls) = extract_urls_from_sitemap(&content);
            
            // Add all page URLs to the result set
            for url in page_urls {
                all_urls.insert(url);
            }
            
            // Add all sub-sitemaps to the stack if not visited yet
            for sub_sitemap in sub_sitemaps {
                if !visited_sitemaps.contains(&sub_sitemap) {
                    visited_sitemaps.insert(sub_sitemap.clone());
                    sitemap_stack.push(sub_sitemap);
                }
            }
        }
        
        Ok(())
    }
}

/// Check if a site is likely JavaScript-dependent
pub fn is_javascript_dependent(html: &str) -> (bool, Vec<&str>) {
    use scraper::{Html, Selector};
    
    let document = Html::parse_document(html);
    let mut reasons = Vec::new();
    
    // 1. Check for noscript warnings
    if let Ok(noscript_selector) = Selector::parse("noscript") {
        for noscript in document.select(&noscript_selector) {
            let content = noscript.inner_html().to_lowercase();
            if content.contains("javascript") || 
               content.contains("enable") || 
               content.contains("script") {
                reasons.push("noscript warning found");
                break;
            }
        }
    }
    
    // 2. Check for common JS framework root elements
    for selector_str in &["#app", "#root", "[ng-app]", "[data-reactroot]", ".vue-app", ".ember-view", ".ember-application"] {
        if let Ok(selector) = Selector::parse(selector_str) {
            if document.select(&selector).next().is_some() {
                reasons.push("JavaScript framework root element found");
                break;
            }
        }
    }
    
    // 3. Check for script tags with framework keywords
    if let Ok(script_selector) = Selector::parse("script[src]") {
        for script in document.select(&script_selector) {
            if let Some(src) = script.value().attr("src") {
                let src_lower = src.to_lowercase();
                if src_lower.contains("react") || 
                   src_lower.contains("vue") || 
                   src_lower.contains("angular") ||
                   src_lower.contains("ember") ||
                   src_lower.contains("webpack") ||
                   src_lower.contains("chunk") {
                    reasons.push("JavaScript framework script found");
                    break;
                }
            }
        }
    }
    
    // 4. Check for meta tags indicating JS frameworks
    if let Ok(meta_selector) = Selector::parse("meta[name='crates-io/config/environment']") {
        if document.select(&meta_selector).next().is_some() {
            reasons.push("Ember.js application detected");
        }
    }
    
    // 5. Check for lazy loaded images
    if let Ok(img_selector) = Selector::parse("img[loading='lazy'], img[data-src]") {
        if document.select(&img_selector).next().is_some() {
            reasons.push("Lazy-loaded images found");
        }
    }
    
    // 6. Check for web components
    if let Ok(component_selector) = Selector::parse("*[is], *[custom-element]") {
        if document.select(&component_selector).next().is_some() {
            reasons.push("Web components found");
        }
    }
    
    // 7. Check for empty content containers
    if let Ok(content_selector) = Selector::parse("main, #content, .content, article") {
        for content in document.select(&content_selector) {
            if content.inner_html().trim().is_empty() {
                reasons.push("Empty content container found");
                break;
            }
        }
    }
    
    // 8. Check for loading indicators
    if let Ok(loading_selector) = Selector::parse("[class*='loading'], [id*='loading'], [class*='spinner']") {
        if document.select(&loading_selector).next().is_some() {
            reasons.push("Loading indicator found");
        }
    }
    
    // 9. Check for specific dynamic content triggers
    if html.contains("window.") || html.contains("document.") || 
       html.contains("addEventListener") || html.contains("DOMContentLoaded") {
        reasons.push("Dynamic content initialization found");
    }
    
    // Consider JS-dependent if 1 or more indicators are present (lowered from 2)
    (reasons.len() >= 1, reasons)
} 