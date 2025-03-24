//! A minimal example of using the robotparser crate that should work

use url::Url;

pub fn example_parser() {
    let robots_txt = r#"
    User-agent: *
    Disallow: /private/
    Allow: /
    
    User-agent: BadBot
    Disallow: /
    "#;
    
    // Try different ways to import
    // Import directly
    match robotparser::RobotFileParser::new("https://www.example.com/robots.txt") {
        Ok(mut parser) => {
            println!("Import directly worked");
            parser.parse(robots_txt);
            let url = "https://www.example.com/public";
            let allowed = parser.can_fetch("MyBot", url);
            println!("Is allowed: {}", allowed);
        }
        Err(_) => println!("Import directly failed"),
    }
    
    // Try with parser module
    match robotparser::parser::RobotFileParser::new("https://www.example.com/robots.txt") {
        Ok(mut parser) => {
            println!("Import from parser module worked");
            parser.parse(robots_txt);
            let url = "https://www.example.com/public";
            let allowed = parser.can_fetch("MyBot", url);
            println!("Is allowed: {}", allowed);
        }
        Err(_) => println!("Import from parser module failed"),
    }
    
    // Try with model module
    match robotparser::model::RobotsData::default() {
        // Default constructor without new
        Ok(mut robots) => {
            println!("Import from model module worked");
            robots.parse(robots_txt);
            let url = Url::parse("https://www.example.com/public").unwrap();
            let allowed = robots.can_fetch("MyBot", &url);
            println!("Is allowed: {}", allowed);
        }
        Err(_) => println!("Import from model module failed"),
    }
} 