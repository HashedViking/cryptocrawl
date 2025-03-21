use axum::{
    routing::{get, post},
    Router, extract::{State, Path, Json}, http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use crate::db::Database;
use crate::models::{Task, TaskStatus, CrawlReport};
use crate::evaluator::Evaluator;
use crate::solana::SolanaIntegration;
use std::sync::atomic::{AtomicBool, Ordering};
use tower_http::cors::{CorsLayer, Any};
use log::info;
use url::Url;

/// Application state
pub struct AppState {
    /// Database connection
    db: Arc<Mutex<Database>>,
    /// Evaluator for report verification
    evaluator: Arc<Evaluator>,
    /// Solana integration
    solana: Arc<SolanaIntegration>,
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
pub struct TaskRequest {
    pub target_url: String,
    pub max_depth: u32,
    pub follow_subdomains: bool,
    pub max_links: Option<u32>,
    pub incentive_amount: Option<u64>,
}

#[derive(Serialize)]
pub struct TaskResponse {
    pub id: String,
    pub target_url: String,
    pub max_depth: u32,
    pub follow_subdomains: bool,
    pub max_links: Option<u32>,
    pub created_at: u64,
    pub status: String,
    pub incentive_amount: u64,
}

#[derive(Serialize, Deserialize)]
pub struct TaskAssignmentRequest {
    pub client_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct CrawlReportSubmission {
    pub task_id: String,
    pub client_id: String,
    pub domain: String,
    pub pages: Vec<PageSubmission>,
    pub start_time: u64,
    pub end_time: u64,
}

#[derive(Serialize, Deserialize)]
pub struct PageSubmission {
    pub url: String,
    pub status: Option<u16>,
    pub content_type: Option<String>,
    pub size: usize,
    pub timestamp: u64,
}

#[derive(Serialize)]
pub struct VerificationResult {
    pub task_id: String,
    pub verified: bool,
    pub score: f64,
    pub notes: String,
    pub transaction_hash: String,
    pub incentive_amount: Option<u64>,
}

#[derive(Serialize)]
pub struct ApiDocResponse {
    pub package: String,
    pub documentation: String,
}

// API implementation
pub async fn start_api_server(
    db: Arc<Database>,
    evaluator: Arc<Evaluator>,
    solana: SolanaIntegration,
    addr: &str,
) -> Result<(), anyhow::Error> {
    // Create shared state
    let state = Arc::new(AppState {
        db: Arc::new(Mutex::new(db.as_ref().clone())),
        evaluator: evaluator.clone(),
        solana: Arc::new(solana),
        running: Arc::new(AtomicBool::new(true)),
    });

    // Configure CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Build router
    let app = Router::new()
        .route("/api/tasks/assign", post(assign_next_task))
        .route("/api/tasks", get(get_all_tasks).post(create_task))
        .route("/api/tasks/:id", get(get_task))
        .route("/api/tasks/:id/assign", post(assign_task))
        .route("/api/reports", post(submit_report))
        .route("/api/reports/:task_id", get(get_report))
        .route("/api/crawlers/register", post(register_crawler))
        .route("/api/docs/:package", get(get_api_docs))
        .route("/api/health", get(health_check))
        .layer(cors)
        .with_state(state);

    // Start server
    info!("Starting API server on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::Server::from_tcp(listener.into_std()?)?.serve(app.into_make_service()).await?;

    Ok(())
}

// API route handlers
async fn health_check(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if state.running.load(Ordering::Relaxed) {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

async fn get_all_tasks(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<TaskResponse>>, ApiError> {
    let db = state.db.lock().await;
    let tasks = db.get_pending_tasks()?;
    
    let task_responses = tasks.into_iter()
        .map(|task| TaskResponse {
            id: task.id,
            target_url: task.target_url,
            max_depth: task.max_depth,
            follow_subdomains: task.follow_subdomains,
            max_links: task.max_links,
            created_at: task.created_at,
            status: format!("{:?}", task.status),
            incentive_amount: task.incentive_amount,
        })
        .collect();
    
    Ok(Json(task_responses))
}

async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Result<Json<TaskResponse>, ApiError> {
    let db = state.db.lock().await;
    let task = db.get_task(&task_id)?
        .ok_or_else(|| ApiError::NotFound(format!("Task {} not found", task_id)))?;
    
    let task_response = TaskResponse {
        id: task.id,
        target_url: task.target_url,
        max_depth: task.max_depth,
        follow_subdomains: task.follow_subdomains,
        max_links: task.max_links,
        created_at: task.created_at,
        status: format!("{:?}", task.status),
        incentive_amount: task.incentive_amount,
    };
    
    Ok(Json(task_response))
}

async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(task_req): Json<TaskRequest>,
) -> Result<Json<TaskResponse>, ApiError> {
    // Validate URL
    if let Err(e) = Url::parse(&task_req.target_url) {
        return Err(ApiError::BadRequest(format!("Invalid URL: {}", e)));
    }
    
    // Create task
    let task_id = Uuid::new_v4().to_string();
    let incentive_amount = task_req.incentive_amount.unwrap_or(25_000_000);
    
    let task = Task::new(
        task_id,
        task_req.target_url.clone(),
        task_req.max_depth,
        task_req.follow_subdomains,
        task_req.max_links,
        incentive_amount,
    );
    
    // Save to database
    let db = state.db.lock().await;
    db.create_task(&task)?;
    
    // Create response
    let task_response = TaskResponse {
        id: task.id,
        target_url: task.target_url,
        max_depth: task.max_depth,
        follow_subdomains: task.follow_subdomains,
        max_links: task.max_links,
        created_at: task.created_at,
        status: format!("{:?}", task.status),
        incentive_amount: task.incentive_amount,
    };
    
    Ok(Json(task_response))
}

async fn assign_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
    Json(req): Json<TaskAssignmentRequest>,
) -> Result<Json<TaskResponse>, ApiError> {
    let db = state.db.lock().await;
    
    // Get the task
    let mut task = db.get_task(&task_id)?
        .ok_or_else(|| ApiError::NotFound(format!("Task {} not found", task_id)))?;
    
    // Check if task is available
    if task.status != TaskStatus::Pending {
        return Err(ApiError::BadRequest(format!("Task {} is not available for assignment", task_id)));
    }
    
    // Assign the task
    task.assign(req.client_id);
    
    // Update in database
    db.update_task(&task)?;
    
    // Create response
    let task_response = TaskResponse {
        id: task.id,
        target_url: task.target_url,
        max_depth: task.max_depth,
        follow_subdomains: task.follow_subdomains,
        max_links: task.max_links,
        created_at: task.created_at,
        status: format!("{:?}", task.status),
        incentive_amount: task.incentive_amount,
    };
    
    Ok(Json(task_response))
}

/// Assign the next available task
async fn assign_next_task(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TaskAssignmentRequest>,
) -> Result<Json<TaskResponse>, ApiError> {
    let db = state.db.lock().await;
    
    // Get all pending tasks
    let tasks = db.get_pending_tasks()?;
    
    // Find the first available task
    let mut task = match tasks.into_iter().next() {
        Some(task) => task,
        None => return Err(ApiError::NotFound("No tasks available for assignment".to_string())),
    };
    
    // Assign the task
    task.assign(req.client_id.clone());
    
    // Update in database
    db.update_task(&task)?;
    
    // Create response
    let task_response = TaskResponse {
        id: task.id,
        target_url: task.target_url,
        max_depth: task.max_depth,
        follow_subdomains: task.follow_subdomains,
        max_links: task.max_links,
        created_at: task.created_at,
        status: format!("{:?}", task.status),
        incentive_amount: task.incentive_amount,
    };
    
    Ok(Json(task_response))
}

async fn submit_report(
    State(state): State<Arc<AppState>>,
    Json(submission): Json<CrawlReportSubmission>,
) -> Result<Json<VerificationResult>, ApiError> {
    // Get task
    let db = state.db.lock().await;
    let mut task = db.get_task(&submission.task_id)?
        .ok_or_else(|| ApiError::NotFound(format!("Task not found: {}", submission.task_id)))?;
    
    // Create report
    let report = CrawlReport {
        task_id: submission.task_id.clone(),
        client_id: submission.client_id.clone(),
        domain: submission.domain.clone(),
        pages_count: submission.pages.len(),
        total_size: submission.pages.iter().map(|p| p.size).sum(),
        pages: submission.pages.iter().map(|p| crate::models::CrawledPage {
            url: p.url.clone(),
            status: p.status,
            content_type: p.content_type.clone(),
            size: p.size,
            timestamp: p.timestamp,
        }).collect(),
        start_time: submission.start_time,
        end_time: Some(submission.end_time),
        verified: false,
        verification_score: None,
        verification_notes: None,
    };
    
    // Save report
    db.save_report(&report)?;
    
    // Mark task as completed
    task.complete();
    db.update_task(&task)?;
    
    // Verify the report
    let evaluator = state.evaluator.clone();
    let (verified, score, notes) = evaluator.verify_report(&report).await?;
    
    // Update verification status
    db.update_report_verification(&submission.task_id, verified, Some(score), Some(notes.clone()))?;
    
    // Record verification on blockchain
    let solana = state.solana.clone();
    let tx_hash = solana.submit_verification_result(
        &submission.task_id,
        &submission.client_id,
        verified,
        score,
    )?;
    
    // If verified, transfer incentives
    let incentive_amount = if verified {
        let incentive = task.incentive_amount;
        solana.transfer_incentives(&submission.client_id, incentive)?;
        Some(incentive)
    } else {
        None
    };
    
    // Create response
    let result = VerificationResult {
        task_id: submission.task_id,
        verified,
        score,
        notes,
        transaction_hash: tx_hash,
        incentive_amount,
    };
    
    Ok(Json(result))
}

async fn get_report(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Result<Json<CrawlReport>, ApiError> {
    let db = state.db.lock().await;
    
    // Get the report
    let report = db.get_report_by_task(&task_id)?
        .ok_or_else(|| ApiError::NotFound(format!("Report for task {} not found", task_id)))?;
    
    Ok(Json(report))
}

async fn get_api_docs(
    State(state): State<Arc<AppState>>,
    Path(package): Path<String>,
) -> Result<Json<ApiDocResponse>, ApiError> {
    // Get API documentation
    let evaluator = state.evaluator.clone();
    let docs = evaluator.get_api_documentation(&package).await?;
    
    let response = ApiDocResponse {
        package,
        documentation: docs,
    };
    
    Ok(Json(response))
}

/// Handle crawler registration
async fn register_crawler(
    State(_state): State<Arc<AppState>>,
    Json(request): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    let client_id = request.get("client_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::BadRequest("Missing client_id".to_string()))?;
    
    // In a real implementation, we'd store the crawler in the database
    // For now, we'll just log it
    info!("Registered crawler with client ID: {}", client_id);
    
    Ok(StatusCode::OK)
} 