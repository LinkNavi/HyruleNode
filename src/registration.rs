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
    // Build client with Tor support
    let proxy_config = crate::proxy::ProxyConfig::from_config(config);
    let client = proxy_config.build_client()?;
    
    // Use the public_address() method from config
    let address = config.public_address();
    
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
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?;
    
    if !response.status().is_success() {
        anyhow::bail!("Registration failed: {}", response.status());
    }
    
    let result: RegisterNodeResponse = response.json().await?;
    tracing::info!("✓ {}", result.message);
    
    Ok(())
}

/// Discover peer nodes from the network
pub async fn discover_peers(config: &NodeConfig) -> anyhow::Result<Vec<PeerNode>> {
    let proxy_config = crate::proxy::ProxyConfig::from_config(config);
    let client = proxy_config.build_client()?;
    
    let url = format!("{}/api/nodes", config.hyrule_server);
    
    let response = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?;
    
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
