// hyrule-node/src/registration.rs
use crate::config::NodeConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct RegisterNodeRequest {
    node_id: String,
    address: String,
    port: i32,
    storage_capacity: i64,
    is_anchor: bool,
}

#[derive(Debug, Deserialize)]
struct RegisterNodeResponse {
    node_id: String,
    message: String,
}

/// Register this node with the Hyrule server
pub async fn register_node(config: &NodeConfig) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    
    // Get local IP address (or use configured address)
let address = config.public_address.clone();
    
    let request = RegisterNodeRequest {
        node_id: config.node_id.clone(),
        address,
        port: config.port as i32,
        storage_capacity: config.storage_capacity as i64,
        is_anchor: config.is_anchor,
    };
    
    let url = format!("{}/api/nodes", config.hyrule_server);
    
    let response = client
        .post(&url)
        .json(&request)
        .send()
        .await?;
    
    if !response.status().is_success() {
        anyhow::bail!("Registration failed: {}", response.status());
    }
    
    let result: RegisterNodeResponse = response.json().await?;
    tracing::info!(" {}", result.message);
    
    Ok(())
}

/// Get local IP address
fn get_local_ip() -> Option<String> {
    // Try to get external IP by connecting to a known server
    use std::net::TcpStream;
    
    if let Ok(stream) = TcpStream::connect("8.8.8.8:80") {
        if let Ok(addr) = stream.local_addr() {
            return Some(addr.ip().to_string());
        }
    }
    
    // Fallback to localhost
    Some("127.0.0.1".to_string())
}

/// Discover peer nodes from the network
pub async fn discover_peers(config: &NodeConfig) -> anyhow::Result<Vec<PeerNode>> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/nodes", config.hyrule_server);
    
    let response = client.get(&url).send().await?;
    
    if !response.status().is_success() {
        anyhow::bail!("Failed to discover peers");
    }
    
    let nodes: Vec<PeerNode> = response.json().await?;
    Ok(nodes)
}

#[derive(Debug, Clone, Deserialize)]
pub struct PeerNode {
    pub node_id: String,
    pub address: String,
    pub port: i32,
    pub is_anchor: i64,
    pub last_seen: String,
}
