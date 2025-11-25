// hyrule-node/src/health.rs
use crate::NodeState;
use serde::Serialize;
use std::time::Duration;
use tokio::time;

#[derive(Debug, Serialize)]
struct HeartbeatRequest {
    node_id: String,
    storage_used: i64,
    hosted_repos: Vec<String>,
}

/// Send periodic heartbeats to the Hyrule server
pub async fn heartbeat_loop(state: NodeState) {
    let mut interval = time::interval(Duration::from_secs(60)); // Every minute
    let mut uptime = 0u64;
    
    loop {
        interval.tick().await;
        uptime += 60;
        
        // Update uptime in stats
        {
            let mut stats = state.stats.write().await;
            stats.uptime_seconds = uptime;
        }
        
        // Send heartbeat
        if let Err(e) = send_heartbeat(&state).await {
            tracing::warn!("Heartbeat failed: {}", e);
        }
        
        // Verify storage integrity periodically (every hour)
        if uptime % 3600 == 0 {
            tokio::spawn({
                let state = state.clone();
                async move {
                    if let Err(e) = verify_all_repos(&state).await {
                        tracing::error!("Storage verification failed: {}", e);
                    }
                }
            });
        }
    }
}

async fn send_heartbeat(state: &NodeState) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    
    let storage_used = state.storage.get_storage_usage()? as i64;
    let hosted_repos = state.hosted_repos.read().await.clone();
    
    let request = HeartbeatRequest {
        node_id: state.config.node_id.clone(),
        storage_used,
        hosted_repos: hosted_repos.clone(),
    };
    
    let url = format!("{}/api/nodes/heartbeat", state.config.hyrule_server);
    
    let response = client
        .post(&url)
        .json(&request)
        .timeout(Duration::from_secs(10))
        .send()
        .await?;
    
    if !response.status().is_success() {
        tracing::warn!("Heartbeat rejected: {}", response.status());
    } else {
        tracing::debug!("Heartbeat sent - hosting {} repos", hosted_repos.len());
    }
    
    Ok(())
}

async fn verify_all_repos(state: &NodeState) -> anyhow::Result<()> {
    tracing::info!(" Starting storage verification...");
    
    let repos = state.hosted_repos.read().await.clone();
    let mut total_objects = 0;
    let mut corrupted = 0;
    
    for repo_hash in repos {
        let objects = state.storage.list_objects(&repo_hash)?;
        total_objects += objects.len();
        
        for object_id in objects {
            match state.storage.verify_object(&repo_hash, &object_id) {
                Ok(true) => {
                    // Object is valid
                }
                Ok(false) | Err(_) => {
                    tracing::warn!("Corrupted object: {}:{}", &repo_hash[..8], &object_id[..8]);
                    corrupted += 1;
                }
            }
        }
    }
    
    if corrupted > 0 {
        tracing::warn!(" Found {} corrupted objects out of {}", corrupted, total_objects);
    } else {
        tracing::info!(" All {} objects verified successfully", total_objects);
    }
    
    Ok(())
}

/// Monitor storage capacity and alert if nearly full
pub async fn monitor_storage(state: NodeState) {
    let mut interval = time::interval(Duration::from_secs(300)); // Every 5 minutes
    
    loop {
        interval.tick().await;
        
        match state.storage.get_storage_usage() {
            Ok(used) => {
                let capacity = state.config.storage_capacity;
                let usage_percent = (used as f64 / capacity as f64) * 100.0;
                
                if usage_percent > 90.0 {
                    tracing::error!(" Storage nearly full: {:.1}%", usage_percent);
                } else if usage_percent > 80.0 {
                    tracing::warn!(" Storage usage high: {:.1}%", usage_percent);
                }
                
                // Update stats
                {
                    let mut stats = state.stats.write().await;
                    stats.repos_hosted = state.hosted_repos.read().await.len();
                }
            }
            Err(e) => {
                tracing::error!("Failed to check storage usage: {}", e);
            }
        }
    }
}
