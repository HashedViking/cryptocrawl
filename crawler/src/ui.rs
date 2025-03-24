use axum::{
    routing::{get, post},
    Router, extract::{State, Path, Json}, http::StatusCode,
    response::{IntoResponse, Response, Html},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use crate::db::Database;
use crate::models::{Task, CrawlResult, CrawlStatus};
use crate::crawler::Crawler;
use crate::solana::SolanaIntegration;
use std::sync::atomic::{AtomicBool, Ordering};
use log::{info, error};
use anyhow::Result;

/// Application state
pub struct AppState {
    /// Database connection
    db: Arc<Mutex<Database>>,
    /// Crawler instance
    crawler: Arc<Mutex<Crawler>>,
    /// Solana integration
    solana: Arc<SolanaIntegration>,
    /// Client ID
    client_id: String,
    /// Whether the system is running
    running: Arc<AtomicBool>,
}

// API Error handling
#[derive(Debug)]
pub enum ApiError {
    InternalError(String),
    NotFound(String),
    BadRequest(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            ApiError::InternalError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
        };

        (status, error_message).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        ApiError::InternalError(err.to_string())
    }
}

// Request and response types
#[derive(Serialize, Deserialize)]
pub struct TaskAssignRequest {
    pub url: String,
}

#[derive(Serialize)]
pub struct WalletResponse {
    pub address: String,
    pub balance: u64,
    pub history: Vec<WalletHistoryItem>,
}

#[derive(Serialize)]
pub struct WalletHistoryItem {
    pub task_id: String,
    pub amount: i64,
    pub timestamp: u64,
    pub transaction_hash: String,
    pub description: Option<String>,
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub client_id: String,
    pub wallet_address: String,
    pub wallet_balance: u64,
    pub active_task: Option<TaskStatus>,
    pub completed_tasks: usize,
}

#[derive(Serialize)]
pub struct TaskStatus {
    pub id: String,
    pub url: String,
    pub status: String,
    pub pages_crawled: usize,
    pub data_size: usize,
}

// Templates
fn index_template(status: &StatusResponse) -> String {
    let active_task_html = match &status.active_task {
        Some(task) => format!(
            r#"
            <div class="card bg-dark text-white mb-4">
                <div class="card-header">Active Task</div>
                <div class="card-body">
                    <p><strong>Task ID:</strong> {}</p>
                    <p><strong>URL:</strong> {}</p>
                    <p><strong>Status:</strong> {}</p>
                    <p><strong>Pages Crawled:</strong> {}</p>
                    <p><strong>Data Size:</strong> {} bytes</p>
                </div>
            </div>
            "#,
            task.id, task.url, task.status, task.pages_crawled, task.data_size
        ),
        None => r#"
            <div class="card bg-dark text-white mb-4">
                <div class="card-header">No Active Task</div>
                <div class="card-body">
                    <p>No crawling task is currently active.</p>
                    <form action="/api/tasks/assign" method="post" class="mt-3">
                        <div class="input-group">
                            <input type="text" name="url" class="form-control" placeholder="Enter URL to crawl">
                            <button type="submit" class="btn btn-primary">Start Crawling</button>
                        </div>
                    </form>
                </div>
            </div>
        "#.to_string(),
    };

    format!(
        r#"
        <!DOCTYPE html>
        <html lang="en">
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <title>CryptoCrawl Client</title>
            <link href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.0-alpha1/dist/css/bootstrap.min.css" rel="stylesheet">
            <style>
                body {{ background-color: #121212; color: #e0e0e0; }}
                .card {{ background-color: #1e1e1e; border-color: #333; }}
                .card-header {{ background-color: #252525; border-color: #333; }}
                .navbar {{ background-color: #252525; }}
            </style>
        </head>
        <body>
            <nav class="navbar navbar-expand-lg navbar-dark mb-4">
                <div class="container">
                    <a class="navbar-brand" href="/">CryptoCrawl Client</a>
                </div>
            </nav>
            
            <div class="container">
                <div class="row mb-4">
                    <div class="col-md-6">
                        <div class="card bg-dark text-white">
                            <div class="card-header">Client Information</div>
                            <div class="card-body">
                                <p><strong>Client ID:</strong> {}</p>
                                <p><strong>Wallet Address:</strong> {}</p>
                                <p><strong>Balance:</strong> {} tokens</p>
                                <p><strong>Completed Tasks:</strong> {}</p>
                            </div>
                        </div>
                    </div>
                    <div class="col-md-6">
                        {}
                    </div>
                </div>
                
                <div class="card bg-dark text-white">
                    <div class="card-header">Task History</div>
                    <div class="card-body">
                        <a href="/tasks" class="btn btn-primary">View Task History</a>
                    </div>
                </div>
            </div>
            
            <script src="https://cdn.jsdelivr.net/npm/bootstrap@5.3.0-alpha1/dist/js/bootstrap.bundle.min.js"></script>
        </body>
        </html>
        "#,
        status.client_id,
        status.wallet_address,
        status.wallet_balance,
        status.completed_tasks,
        active_task_html
    )
}

fn tasks_template(tasks: &Vec<CrawlResult>) -> String {
    let task_rows = tasks
        .iter()
        .map(|task| {
            let status_class = match task.status {
                CrawlStatus::Completed => "text-success",
                CrawlStatus::Failed => "text-danger",
                CrawlStatus::Verified => "text-primary",
                CrawlStatus::Rejected => "text-warning",
                _ => "text-secondary",
            };
            
            let incentives = match task.incentives_received {
                Some(amount) => format!("{} tokens", amount),
                None => "N/A".to_string(),
            };
            
            let end_time = match task.end_time {
                Some(time) => {
                    let duration = time - task.start_time;
                    format!("{} seconds", duration)
                },
                None => "In progress".to_string(),
            };
            
            format!(
                r#"
                <tr>
                    <td><a href="/tasks/{}" class="text-info">{}</a></td>
                    <td>{}</td>
                    <td class="{}">{:?}</td>
                    <td>{}</td>
                    <td>{}</td>
                    <td>{}</td>
                    <td>{}</td>
                </tr>
                "#,
                task.task_id,
                task.task_id,
                task.domain,
                status_class,
                task.status,
                task.pages_count,
                task.total_size as usize,
                end_time,
                incentives
            )
        })
        .collect::<Vec<String>>()
        .join("");

    format!(
        r#"
        <!DOCTYPE html>
        <html lang="en">
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <title>Task History - CryptoCrawl Client</title>
            <link href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.0-alpha1/dist/css/bootstrap.min.css" rel="stylesheet">
            <style>
                body {{ background-color: #121212; color: #e0e0e0; }}
                .card {{ background-color: #1e1e1e; border-color: #333; }}
                .card-header {{ background-color: #252525; border-color: #333; }}
                .navbar {{ background-color: #252525; }}
                th, td {{ color: #e0e0e0; }}
                .table {{ color: #e0e0e0; }}
            </style>
        </head>
        <body>
            <nav class="navbar navbar-expand-lg navbar-dark mb-4">
                <div class="container">
                    <a class="navbar-brand" href="/">CryptoCrawl Client</a>
                </div>
            </nav>
            
            <div class="container">
                <h2 class="mb-4">Task History</h2>
                
                <div class="card bg-dark text-white mb-4">
                    <div class="card-body">
                        <div class="table-responsive">
                            <table class="table table-dark">
                                <thead>
                                    <tr>
                                        <th>Task ID</th>
                                        <th>Domain</th>
                                        <th>Status</th>
                                        <th>Pages</th>
                                        <th>Data Size</th>
                                        <th>Duration</th>
                                        <th>Incentives</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {}
                                </tbody>
                            </table>
                        </div>
                    </div>
                </div>
                
                <a href="/" class="btn btn-primary">Back to Dashboard</a>
            </div>
            
            <script src="https://cdn.jsdelivr.net/npm/bootstrap@5.3.0-alpha1/dist/js/bootstrap.bundle.min.js"></script>
        </body>
        </html>
        "#,
        task_rows
    )
}

fn task_detail_template(task: &CrawlResult) -> String {
    let status_class = match task.status {
        CrawlStatus::Completed => "text-success",
        CrawlStatus::Failed => "text-danger",
        CrawlStatus::Verified => "text-primary",
        CrawlStatus::Rejected => "text-warning",
        _ => "text-secondary",
    };
    
    let incentives = match task.incentives_received {
        Some(amount) => format!("{} tokens", amount),
        None => "N/A".to_string(),
    };
    
    let transaction_hash = match &task.transaction_hash {
        Some(hash) => hash,
        None => "N/A",
    };
    
    let page_rows = task.pages.iter().enumerate()
        .map(|(i, page)| {
            format!(
                r#"
                <tr>
                    <td>{}</td>
                    <td>{}</td>
                    <td>{}</td>
                    <td>{}</td>
                </tr>
                "#,
                i + 1,
                page.url,
                page.size,
                page.timestamp
            )
        })
        .collect::<Vec<String>>()
        .join("");

    format!(
        r#"
        <!DOCTYPE html>
        <html lang="en">
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <title>Task Detail - CryptoCrawl Client</title>
            <link href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.0-alpha1/dist/css/bootstrap.min.css" rel="stylesheet">
            <style>
                body {{ background-color: #121212; color: #e0e0e0; }}
                .card {{ background-color: #1e1e1e; border-color: #333; }}
                .card-header {{ background-color: #252525; border-color: #333; }}
                .navbar {{ background-color: #252525; }}
                th, td {{ color: #e0e0e0; }}
                .table {{ color: #e0e0e0; }}
            </style>
        </head>
        <body>
            <nav class="navbar navbar-expand-lg navbar-dark mb-4">
                <div class="container">
                    <a class="navbar-brand" href="/">CryptoCrawl Client</a>
                </div>
            </nav>
            
            <div class="container">
                <h2 class="mb-4">Task Detail</h2>
                
                <div class="card bg-dark text-white mb-4">
                    <div class="card-header">
                        <h4>Task Information</h4>
                    </div>
                    <div class="card-body">
                        <div class="row">
                            <div class="col-md-6">
                                <p><strong>Task ID:</strong> {}</p>
                                <p><strong>Domain:</strong> {}</p>
                                <p><strong>Status:</strong> <span class="{}">{:?}</span></p>
                                <p><strong>Pages Crawled:</strong> {}</p>
                            </div>
                            <div class="col-md-6">
                                <p><strong>Data Size:</strong> {} bytes</p>
                                <p><strong>Transaction Hash:</strong> {}</p>
                                <p><strong>Incentives Received:</strong> {}</p>
                            </div>
                        </div>
                    </div>
                </div>
                
                <div class="card bg-dark text-white mb-4">
                    <div class="card-header">
                        <h4>Crawled Pages</h4>
                    </div>
                    <div class="card-body">
                        <div class="table-responsive">
                            <table class="table table-dark">
                                <thead>
                                    <tr>
                                        <th>#</th>
                                        <th>URL</th>
                                        <th>Size</th>
                                        <th>Timestamp</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {}
                                </tbody>
                            </table>
                        </div>
                    </div>
                </div>
                
                <a href="/tasks" class="btn btn-primary">Back to Task History</a>
            </div>
            
            <script src="https://cdn.jsdelivr.net/npm/bootstrap@5.3.0-alpha1/dist/js/bootstrap.bundle.min.js"></script>
        </body>
        </html>
        "#,
        task.task_id,
        task.domain,
        status_class,
        task.status,
        task.pages_count,
        task.total_size as usize,
        transaction_hash,
        incentives,
        page_rows
    )
}

// Start the UI server
pub async fn start_ui_server(
    db: Database,
    crawler: Crawler,
    solana: SolanaIntegration,
    addr: &str,
    client_id: &str,
) -> Result<()> {
    // Create shared state
    let state = Arc::new(AppState {
        db: Arc::new(Mutex::new(db)),
        crawler: Arc::new(Mutex::new(crawler)),
        solana: Arc::new(solana),
        client_id: client_id.to_string(),
        running: Arc::new(AtomicBool::new(true)),
    });

    // Build router with routes and state
    let app = Router::new()
        .route("/", get(index_page))
        .route("/tasks", get(tasks_page))
        .route("/tasks/:id", get(task_detail_page))
        .route("/api/tasks/assign", post(assign_task))
        .route("/api/wallet", get(get_wallet))
        .route("/api/status", get(get_status))
        .route("/api/health", get(health_check))
        .with_state(state);

    // Start server
    info!("Starting UI server on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// Route handlers
async fn health_check(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if state.running.load(Ordering::Relaxed) {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

async fn index_page(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, ApiError> {
    let status = get_status_data(state).await?;
    let html = index_template(&status);
    Ok(Html(html))
}

async fn tasks_page(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, ApiError> {
    let db = state.db.lock().await;
    let tasks = db.get_all_crawl_results()?;
    let html = tasks_template(&tasks);
    Ok(Html(html))
}

async fn task_detail_page(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Result<Html<String>, ApiError> {
    let db = state.db.lock().await;
    let task = db.get_crawl_result(&task_id)?
        .ok_or_else(|| ApiError::NotFound(format!("Task {} not found", task_id)))?;
    
    let html = task_detail_template(&task);
    Ok(Html(html))
}

async fn assign_task(
    State(state): State<Arc<AppState>>,
    form: axum::extract::Form<TaskAssignRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // Create a new task
    let task_id = Uuid::new_v4().to_string();
    let task = Task {
        id: task_id,
        target_url: form.url.clone(),
        max_depth: 2,
        follow_subdomains: false,
        max_links: Some(100),
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        assigned_at: Some(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()),
        incentive_amount: 25_000_000,
    };
    
    // Save task to database
    {
        let db = state.db.lock().await;
        db.save_task(&task)?;
    }
    
    // Start crawling in a background task
    let state_clone = state.clone();
    tokio::spawn(async move {
        let crawl_result = {
            let mut crawler = state_clone.crawler.lock().await;
            *crawler = Crawler::new(task.clone());
            match crawler.crawl_current().await {
                Ok(result) => result,
                Err(e) => {
                    error!("Crawl failed: {}", e);
                    return;
                }
            }
        };
        
        // Save result to database
        let db = state_clone.db.lock().await;
        match db.save_crawl_result(&crawl_result) {
            Ok(_) => info!("Saved crawl result for task {}", crawl_result.task_id),
            Err(e) => error!("Failed to save crawl result: {}", e),
        }
        
        // Update result with blockchain submission
        let task_id = crawl_result.task_id.clone();
        let _domain = crawl_result.domain.clone();
        let _pages_count = crawl_result.pages_count;
        let _total_size = crawl_result.total_size;

        // Clone objects needed inside async block
        let solana_clone = state_clone.solana.clone();
        let db_clone = state_clone.db.clone();

        // Run in a separate task to avoid blocking
        tokio::spawn(async move {
            // Log the transaction
            info!("Submitting crawl data to blockchain for task {}", task_id);
            
            // Submit to blockchain
            match solana_clone.submit_crawl_report(&task_id, &crawl_result).await {
                Ok(tx_hash) => {
                    info!("Submitted crawl data to blockchain: {}", tx_hash);
                    
                    // Update result with transaction hash
                    let mut updated_result = crawl_result.clone();
                    updated_result.set_transaction(tx_hash.clone());
                    
                    // Claim incentives
                    match solana_clone.claim_incentives(&tx_hash) {
                        Ok(amount) => {
                            info!("Claimed {} incentive tokens", amount);
                            
                            // Update database - tokio's Mutex.lock() returns MutexGuard directly, not Result
                            let db_guard = db_clone.lock().await;
                            if let Err(e) = db_guard.update_crawl_result(&updated_result) {
                                error!("Failed to update crawl result with transaction: {}", e);
                            }
                            
                            if let Err(e) = db_guard.add_wallet_history(
                                &task_id, 
                                amount, 
                                &tx_hash, 
                                Some("Incentive claim")
                            ) {
                                error!("Failed to add wallet history entry: {}", e);
                            }
                        }
                        Err(e) => error!("Failed to claim incentives: {}", e),
                    }
                }
                Err(e) => error!("Failed to submit crawl data to blockchain: {}", e),
            }
        });
    });
    
    // Redirect to home page
    Ok((StatusCode::SEE_OTHER, [("Location", "/")]))
}

async fn get_wallet(
    State(state): State<Arc<AppState>>,
) -> Result<Json<WalletResponse>, ApiError> {
    let solana = &state.solana;
    let wallet_address = solana.get_wallet_address();
    let balance = solana.get_balance()?;
    
    let db = state.db.lock().await;
    let history = db.get_wallet_history(Some(10))?;
    
    let history_items = history.into_iter()
        .map(|(task_id, amount, timestamp, tx_hash, description)| WalletHistoryItem {
            task_id,
            amount,
            timestamp,
            transaction_hash: tx_hash,
            description,
        })
        .collect();
    
    let response = WalletResponse {
        address: wallet_address,
        balance,
        history: history_items,
    };
    
    Ok(Json(response))
}

async fn get_status_data(
    state: Arc<AppState>,
) -> Result<StatusResponse, ApiError> {
    // Get wallet info
    let solana = &state.solana;
    let wallet_address = solana.get_wallet_address();
    let wallet_balance = solana.get_balance()?;
    
    // Get active task if any
    let crawler_guard = state.crawler.lock().await;
    let db_guard = state.db.lock().await;
    let active_task = if let Some(task) = crawler_guard.current_task() {
        // Check for crawl result
        if let Ok(Some(result)) = db_guard.get_crawl_result(&task.id) {
            // Task is in progress or completed
            Some(TaskStatus {
                id: task.id.clone(),
                url: task.target_url.clone(),
                status: result.status.to_string(),
                pages_crawled: result.pages_count,
                data_size: result.total_size as usize,
            })
        } else {
            // Task exists but no result yet
            Some(TaskStatus {
                id: task.id.clone(),
                url: task.target_url.clone(),
                status: "Ready".to_string(),
                pages_crawled: 0,
                data_size: 0,
            })
        }
    } else {
        None
    };
    
    // Get completed tasks count
    let completed_tasks = 0; // Placeholder, would get this from the database in a real implementation
    
    Ok(StatusResponse {
        client_id: state.client_id.clone(),
        wallet_address,
        wallet_balance,
        active_task,
        completed_tasks,
    })
}

async fn get_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<StatusResponse>, ApiError> {
    let status = get_status_data(state).await?;
    Ok(Json(status))
} 