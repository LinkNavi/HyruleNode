// ============================================================================
// Node/src/api.rs - Enhanced API with Batch Operations
// ============================================================================

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Router, Json,
};
use serde::{Deserialize, Serialize};
use crate::NodeState;

#[derive(Debug, Serialize)]
struct StatusResponse {
    node_id: String,
    uptime_seconds: u64,
    storage_used: u64,
    storage_capacity: u64,
    repos_hosted: usize,
    total_requests: u64,
    bytes_served: u64,
    is_anchor: bool,
    replication_count: u64,
    failed_requests: u64,
    features: NodeFeatures,
}

#[derive(Debug, Serialize)]
struct NodeFeatures {
    dht_enabled: bool,
    proxy_enabled: bool,
    auto_replicate: bool,
}

#[derive(Debug, Deserialize)]
struct StoreObjectRequest {
    object_id: String,
    data: String,
}

#[derive(Debug, Serialize)]
struct StoreObjectResponse {
    success: bool,
    object_id: String,
}

#[derive(Debug, Deserialize)]
struct BatchStoreRequest {
    objects: Vec<StoreObjectRequest>,
}

#[derive(Debug, Serialize)]
struct BatchStoreResponse {
    uploaded: usize,
    failed: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateRefRequest {
    ref_name: String,
    commit_id: String,
}

#[derive(Debug, Serialize)]
struct ListObjectsResponse {
    objects: Vec<String>,
    count: usize,
}

pub fn create_router(state: NodeState) -> Router {
    Router::new()
        .route("/status", get(get_status))
        .route("/health", get(health_check))
        .route("/repos", get(list_repos))
        .route("/repos/{hash}/objects/{id}", get(get_object))
        .route("/repos/{hash}/objects", post(store_object))
        .route("/repos/{hash}/objects", get(list_objects))
        .route("/repos/{hash}/objects/batch", post(batch_store_objects))
        .route("/repos/{hash}/refs", post(update_ref))
        .route("/repos/{hash}/refs/{ref_name}", get(get_ref))
        .route("/repos/{hash}/init", post(init_repo))
        .route("/repos/{hash}/pack", get(get_packfile))
        .with_state(state)
}
async fn get_status(
    State(state): State<NodeState>,
) -> Result<Json<StatusResponse>, StatusCode> {
    let stats = state.stats.read().await;
    let storage_used = state.storage.get_storage_usage()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    let repos = state.hosted_repos.read().await;
    
    let features = NodeFeatures {
        dht_enabled: state.dht.read().await.is_some(),
        proxy_enabled: state.config.enable_proxy,
        auto_replicate: state.config.auto_replicate,
    };
    
    Ok(Json(StatusResponse {
        node_id: state.config.node_id.clone(),
        uptime_seconds: stats.uptime_seconds,
        storage_used,
        storage_capacity: state.config.storage_capacity,
        repos_hosted: repos.len(),
        total_requests: stats.total_requests,
        bytes_served: stats.bytes_served,
        is_anchor: state.config.is_anchor,
        replication_count: stats.replication_count,
        failed_requests: stats.failed_requests,
        features,
    }))
}

async fn health_check() -> StatusCode {
    StatusCode::OK
}

async fn list_repos(
    State(state): State<NodeState>,
) -> Result<Json<Vec<String>>, StatusCode> {
    let repos = state.hosted_repos.read().await;
    Ok(Json(repos.clone()))
}

async fn get_object(
    State(state): State<NodeState>,
    Path((repo_hash, object_id)): Path<(String, String)>,
) -> Result<Vec<u8>, StatusCode> {
    {
        let mut stats = state.stats.write().await;
        stats.total_requests += 1;
    }
    
    let data = state.storage
        .read_object(&repo_hash, &object_id)
        .map_err(|_| {
            let mut stats = futures::executor::block_on(state.stats.write());
            stats.failed_requests += 1;
            StatusCode::NOT_FOUND
        })?;
    
    {
        let mut stats = state.stats.write().await;
        stats.bytes_served += data.len() as u64;
    }
    
    Ok(data)
}

async fn store_object(
    State(state): State<NodeState>,
    Path(repo_hash): Path<String>,
    Json(payload): Json<StoreObjectRequest>,
) -> Result<Json<StoreObjectResponse>, StatusCode> {
    use base64::{Engine as _, engine::general_purpose};
    
    let data = general_purpose::STANDARD
        .decode(&payload.data)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    
    state.storage
        .store_object(&repo_hash, &payload.object_id, &data)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    {
        let mut repos = state.hosted_repos.write().await;
        if !repos.contains(&repo_hash) {
            repos.push(repo_hash.clone());
        }
    }
    
    Ok(Json(StoreObjectResponse {
        success: true,
        object_id: payload.object_id,
    }))
}

async fn batch_store_objects(
    State(state): State<NodeState>,
    Path(repo_hash): Path<String>,
    Json(payload): Json<BatchStoreRequest>,
) -> Result<Json<BatchStoreResponse>, StatusCode> {
    use base64::{Engine as _, engine::general_purpose};
    
    let mut uploaded = 0;
    let mut failed = Vec::new();
    
    for obj in payload.objects {
        match general_purpose::STANDARD.decode(&obj.data) {
            Ok(data) => {
                if state.storage.store_object(&repo_hash, &obj.object_id, &data).is_ok() {
                    uploaded += 1;
                } else {
                    failed.push(obj.object_id);
                }
            }
            Err(_) => {
                failed.push(obj.object_id);
            }
        }
    }
    
    {
        let mut repos = state.hosted_repos.write().await;
        if !repos.contains(&repo_hash) {
            repos.push(repo_hash);
        }
    }
    
    Ok(Json(BatchStoreResponse { uploaded, failed }))
}

async fn list_objects(
    State(state): State<NodeState>,
    Path(repo_hash): Path<String>,
) -> Result<Json<ListObjectsResponse>, StatusCode> {
    let objects = state.storage
        .list_objects(&repo_hash)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    let count = objects.len();
    
    Ok(Json(ListObjectsResponse { objects, count }))
}

async fn update_ref(
    State(state): State<NodeState>,
    Path(repo_hash): Path<String>,
    Json(payload): Json<UpdateRefRequest>,
) -> Result<StatusCode, StatusCode> {
    state.storage
        .update_ref(&repo_hash, &payload.ref_name, &payload.commit_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(StatusCode::OK)
}

async fn get_ref(
    State(state): State<NodeState>,
    Path((repo_hash, ref_name)): Path<(String, String)>,
) -> Result<String, StatusCode> {
    let decoded_ref = urlencoding::decode(&ref_name)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    
    let commit_id = state.storage
        .read_ref(&repo_hash, &decoded_ref)
        .map_err(|_| StatusCode::NOT_FOUND)?;
    
    Ok(commit_id)
}

async fn init_repo(
    State(state): State<NodeState>,
    Path(repo_hash): Path<String>,
) -> Result<StatusCode, StatusCode> {
    state.storage
        .init_repo(&repo_hash)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    {
        let mut repos = state.hosted_repos.write().await;
        if !repos.contains(&repo_hash) {
            repos.push(repo_hash);
        }
    }
    
    Ok(StatusCode::CREATED)
}

async fn get_packfile(
    State(state): State<NodeState>,
    Path(repo_hash): Path<String>,
) -> Result<Vec<u8>, StatusCode> {
    let pack_data = state.storage
        .create_pack(&repo_hash)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    {
        let mut stats = state.stats.write().await;
        stats.bytes_served += pack_data.len() as u64;
    }
    
    Ok(pack_data)
}


