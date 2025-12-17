use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use super::api_token::ApiTokenManager;
use super::pairing::{PairingManager, PairingRequest, PairingResult};

/// Shared state for the HTTP API
pub struct ApiState {
    pub token_manager: ApiTokenManager,
    pub pairing_manager: PairingManager,
    pub device_id: String,
    pub device_name: String,
    pub public_key: String,
}

/// Status response
#[derive(Serialize)]
struct StatusResponse {
    status: String,
    device_id: String,
    device_name: String,
    version: String,
}

/// Pairing session created response
#[derive(Serialize)]
struct PairingSessionResponse {
    code: String,
    expires_in_seconds: u64,
}

/// Join pairing request
#[derive(Deserialize)]
struct JoinPairingRequest {
    code: String,
    device_id: String,
    device_name: String,
    public_key: String,
}

/// Join pairing response
#[derive(Serialize)]
struct JoinPairingResponse {
    status: String,
    device_id: Option<String>,
    device_name: Option<String>,
    public_key: Option<String>,
}

/// Pending pairing request response
#[derive(Serialize)]
struct PendingRequestResponse {
    pending: bool,
    request: Option<PairingRequestInfo>,
}

#[derive(Serialize)]
struct PairingRequestInfo {
    device_id: String,
    device_name: String,
    public_key_fingerprint: String,
}

/// Accept/reject pairing request
#[derive(Deserialize)]
struct RespondToPairingRequest {
    accept: bool,
}

/// Create the HTTP API router
pub fn create_router(state: Arc<RwLock<ApiState>>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/status", get(get_status))
        .route("/pair/initiate", post(initiate_pairing))
        .route("/pair/join", post(join_pairing))
        .route("/pair/pending", get(get_pending_request))
        .route("/pair/respond", post(respond_to_pairing))
        .route("/pair/cancel", post(cancel_pairing))
        .with_state(state)
}

/// Start the HTTP API server
pub async fn start_server(state: Arc<RwLock<ApiState>>, port: u16) -> anyhow::Result<()> {
    let app = create_router(state);

    // Only bind to localhost for security
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    info!("HTTP API listening on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

/// Validate API token from request headers
fn validate_token(headers: &HeaderMap, state: &ApiState) -> Result<(), StatusCode> {
    let token = headers
        .get("X-Wolfpack-Token")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if !state.token_manager.validate(token) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(())
}

/// Check origin header for CSRF protection
fn check_origin(headers: &HeaderMap) -> Result<(), StatusCode> {
    // Allow requests with no origin (CLI tools, curl)
    let origin = match headers.get(header::ORIGIN) {
        Some(o) => o.to_str().unwrap_or(""),
        None => return Ok(()),
    };

    // Allow moz-extension:// origins (Firefox/LibreWolf extensions)
    if origin.starts_with("moz-extension://") {
        return Ok(());
    }

    // Allow chrome-extension:// origins (Chromium-based, if ever supported)
    if origin.starts_with("chrome-extension://") {
        return Ok(());
    }

    // Reject web origins
    warn!("Rejected request from origin: {}", origin);
    Err(StatusCode::FORBIDDEN)
}

// --- Route handlers ---

async fn health_check() -> impl IntoResponse {
    "OK"
}

async fn get_status(
    headers: HeaderMap,
    State(state): State<Arc<RwLock<ApiState>>>,
) -> Result<Json<StatusResponse>, StatusCode> {
    let state = state.read().await;
    check_origin(&headers)?;
    validate_token(&headers, &state)?;

    Ok(Json(StatusResponse {
        status: "running".to_string(),
        device_id: state.device_id.clone(),
        device_name: state.device_name.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }))
}

async fn initiate_pairing(
    headers: HeaderMap,
    State(state): State<Arc<RwLock<ApiState>>>,
) -> Result<Json<PairingSessionResponse>, StatusCode> {
    let state = state.read().await;
    check_origin(&headers)?;
    validate_token(&headers, &state)?;

    let code = state
        .pairing_manager
        .create_session()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(PairingSessionResponse {
        code,
        expires_in_seconds: 300,
    }))
}

async fn join_pairing(
    headers: HeaderMap,
    State(state): State<Arc<RwLock<ApiState>>>,
    Json(req): Json<JoinPairingRequest>,
) -> Result<Json<JoinPairingResponse>, StatusCode> {
    let state = state.read().await;
    check_origin(&headers)?;
    validate_token(&headers, &state)?;

    let pairing_req = PairingRequest {
        device_id: req.device_id,
        device_name: req.device_name,
        public_key: req.public_key,
    };

    let result = state
        .pairing_manager
        .join_session(req.code, pairing_req)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let response = match result {
        PairingResult::Accepted(resp) => JoinPairingResponse {
            status: "accepted".to_string(),
            device_id: Some(resp.device_id),
            device_name: Some(resp.device_name),
            public_key: Some(resp.public_key),
        },
        PairingResult::Rejected => JoinPairingResponse {
            status: "rejected".to_string(),
            device_id: None,
            device_name: None,
            public_key: None,
        },
        PairingResult::Expired => JoinPairingResponse {
            status: "expired".to_string(),
            device_id: None,
            device_name: None,
            public_key: None,
        },
        PairingResult::InvalidCode => JoinPairingResponse {
            status: "invalid_code".to_string(),
            device_id: None,
            device_name: None,
            public_key: None,
        },
    };

    Ok(Json(response))
}

async fn get_pending_request(
    headers: HeaderMap,
    State(state): State<Arc<RwLock<ApiState>>>,
) -> Result<Json<PendingRequestResponse>, StatusCode> {
    let state = state.read().await;
    check_origin(&headers)?;
    validate_token(&headers, &state)?;

    let pending = state
        .pairing_manager
        .get_pending_request()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let response = match pending {
        Some(req) => PendingRequestResponse {
            pending: true,
            request: Some(PairingRequestInfo {
                device_id: req.device_id,
                device_name: req.device_name,
                public_key_fingerprint: fingerprint(&req.public_key),
            }),
        },
        None => PendingRequestResponse {
            pending: false,
            request: None,
        },
    };

    Ok(Json(response))
}

async fn respond_to_pairing(
    headers: HeaderMap,
    State(state): State<Arc<RwLock<ApiState>>>,
    Json(req): Json<RespondToPairingRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let state = state.read().await;
    check_origin(&headers)?;
    validate_token(&headers, &state)?;

    let response = if req.accept {
        Some(super::pairing::PairingResponse {
            device_id: state.device_id.clone(),
            device_name: state.device_name.clone(),
            public_key: state.public_key.clone(),
        })
    } else {
        None
    };

    state
        .pairing_manager
        .respond(req.accept, response)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "status": "ok" })))
}

async fn cancel_pairing(
    headers: HeaderMap,
    State(state): State<Arc<RwLock<ApiState>>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let state = state.read().await;
    check_origin(&headers)?;
    validate_token(&headers, &state)?;

    state
        .pairing_manager
        .cancel()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "status": "ok" })))
}

/// Generate a short fingerprint from a public key
fn fingerprint(public_key: &str) -> String {
    if public_key.len() >= 16 {
        format!(
            "{}...{}",
            &public_key[..8],
            &public_key[public_key.len() - 8..]
        )
    } else {
        public_key.to_string()
    }
}
