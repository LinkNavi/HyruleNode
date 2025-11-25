use crate::{registration, NodeState};
use anyhow::Context;
use std::time::Duration;
use tokio::time;
use bytes::Bytes;

/// Replication loop runs periodically and attempts to replicate unhealthy repos
pub async fn replication_loop(state: NodeState) {
    let mut interval = time::interval(Duration::from_secs(300)); // every 5 minutes

    loop {
        interval.tick().await;

        if !state.config.auto_replicate {
            continue;
        }

        if let Err(e) = check_and_replicate(&state).await {
            tracing::warn!("Replication check failed: {}", e);
        }
    }
}

async fn check_and_replicate(state: &NodeState) -> anyhow::Result<()> {
    let proxy_config = crate::proxy::ProxyConfig::from_config(&state.config);
    // build_client() returns your HyruleClient wrapper
    let client = proxy_config.build_client()?; // HyruleClient

    // get list of unhealthy repos from server
    let url = format!("{}/api/repos?unhealthy=true", state.config.hyrule_server);
    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        // nothing to do
        return Ok(());
    }

    let unhealthy_repos: Vec<String> = response.json().await?;

    if unhealthy_repos.is_empty() {
        return Ok(());
    }

    tracing::info!(
        "Found {} repositories needing replication",
        unhealthy_repos.len()
    );

    // Get current storage usage and available space
    let storage_used = state.storage.get_storage_usage()?;
    let storage_available = state.config.storage_capacity.saturating_sub(storage_used);

    // snapshot hosted repos
    let hosted = state.hosted_repos.read().await.clone();

    for repo_hash in unhealthy_repos {
        if hosted.contains(&repo_hash) {
            continue;
        }

        match get_repo_size(&state.config.hyrule_server, &repo_hash, &client).await {
            Ok(size) => {
                if size > storage_available {
                    tracing::warn!("Not enough space for repo {}", &repo_hash[..8]);
                    continue;
                }

                match replicate_repo(state, &repo_hash, &client).await {
                    Ok(_) => {
                        tracing::info!("Successfully replicated {}", &repo_hash[..8]);

                        // Update stats
                        {
                            let mut stats = state.stats.write().await;
                            stats.replication_count += 1;
                        }

                        let _ = announce_replica(
                            &state.config.hyrule_server,
                            &state.config.node_id,
                            &repo_hash,
                            &client,
                        )
                        .await;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to replicate {}: {}", &repo_hash[..8], e);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to get size for {}: {}", &repo_hash[..8], e);
            }
        }
    }

    Ok(())
}

async fn announce_replica(
    server: &str,
    node_id: &str,
    repo_hash: &str,
    client: &crate::http_client::HyruleClient,
) -> anyhow::Result<()> {
    let url = format!("{}/api/repos/{}/replicate", server, repo_hash);

    #[derive(serde::Serialize)]
    struct AnnounceRequest {
        node_id: String,
    }

    let request = AnnounceRequest {
        node_id: node_id.to_string(),
    };

    let response = client.post(&url).json(&request).send().await?;

    if !response.status().is_success() {
        tracing::warn!("Failed to announce replica: {}", response.status());
    }

    Ok(())
}

async fn replicate_repo(
    state: &NodeState,
    repo_hash: &str,
    client: &crate::http_client::HyruleClient,
) -> anyhow::Result<()> {
    tracing::info!("Starting replication of {}...", &repo_hash[..8]);

    let peers = get_repo_nodes(&state.config.hyrule_server, repo_hash, client).await?;

    if peers.is_empty() {
        anyhow::bail!("No nodes hosting this repository");
    }

    // Try each peer until successful
    for peer in peers.iter() {
        match fetch_repo_from_peer(state, repo_hash, peer, client).await {
            Ok(_) => {
                // Add to hosted repos
                let mut repos = state.hosted_repos.write().await;
                if !repos.contains(&repo_hash.to_string()) {
                    repos.push(repo_hash.to_string());
                }
                return Ok(());
            }
            Err(e) => {
                tracing::warn!("Failed to fetch from peer {}: {}", &peer.node_id[..8], e);
                continue;
            }
        }
    }

    anyhow::bail!("Failed to replicate from all peers")
}

async fn fetch_repo_from_peer(
    state: &NodeState,
    repo_hash: &str,
    peer: &registration::PeerNode,
    client: &crate::http_client::HyruleClient,
) -> anyhow::Result<()> {
    let peer_url = format!("http://{}:{}", peer.address, peer.port);

    // Initialize repo locally
    state.storage.init_repo(repo_hash)?;

    // Get list of objects from peer (JSON)
    let objects_url = format!("{}/repos/{}/objects", peer_url, repo_hash);
    let response = client.get(&objects_url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to get object list: {}", response.status());
    }

    #[derive(serde::Deserialize)]
    struct ObjectList {
        objects: Vec<String>,
    }

    let obj_list: ObjectList = response.json().await?;

    tracing::info!("Fetching {} objects from peer...", obj_list.objects.len());

    // We'll use a plain reqwest::Client to fetch raw object bytes.
    // (Reason: your HyruleResponse wrapper does not expose `.bytes()`.)
    // This bypasses any special behavior HyruleClient applies (tor/proxy). If you need
    // Tor/proxy for object downloads, we can add a `get_raw_bytes` helper on HyruleClient.
    let raw_client = reqwest::Client::new();

    for object_id in obj_list.objects {
        let obj_url = format!("{}/repos/{}/objects/{}", peer_url, repo_hash, object_id);

        match raw_client.get(&obj_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let data: Bytes = resp
                    .bytes()
                    .await
                    .context("reading object bytes from peer")?;
                state
                    .storage
                    .store_object(repo_hash, &object_id, data.as_ref())?;
            }
            Ok(resp) => {
                tracing::warn!(
                    "Failed to fetch object {}: {}",
                    &object_id[..8],
                    resp.status()
                );
            }
            Err(e) => {
                tracing::warn!("Error fetching object {}: {}", &object_id[..8], e);
            }
        }
    }

    tracing::info!("Completed replication from peer {}", &peer.node_id[..8]);
    Ok(())
}

async fn get_repo_size(
    server: &str,
    repo_hash: &str,
    client: &crate::http_client::HyruleClient,
) -> anyhow::Result<u64> {
    let url = format!("{}/api/repos/{}", server, repo_hash);
    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to get repo info");
    }

    #[derive(serde::Deserialize)]
    struct RepoInfo {
        size: i64,
    }

    let info: RepoInfo = response.json().await?;
    Ok(info.size as u64)
}

async fn get_repo_nodes(
    server: &str,
    repo_hash: &str,
    client: &crate::http_client::HyruleClient,
) -> anyhow::Result<Vec<registration::PeerNode>> {
    let url = format!("{}/api/repos/{}/nodes", server, repo_hash);
    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to get repo nodes");
    }

    #[derive(serde::Deserialize)]
    struct NodeInfo {
        node_id: String,
        address: String,
        port: i32,
        is_anchor: bool,
    }

    let nodes: Vec<NodeInfo> = response.json().await?;

    let peers = nodes
        .into_iter()
        .map(|n| registration::PeerNode {
            node_id: n.node_id,
            address: n.address,
            port: n.port,
            is_anchor: if n.is_anchor { 1 } else { 0 },
            last_seen: chrono::Utc::now().to_rfc3339(),
        })
        .collect();

    Ok(peers)
}
