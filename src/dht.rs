// ============================================================================
// Node/src/dht.rs - DHT Implementation
// ============================================================================

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Simple DHT for content discovery
pub struct DHT {
    node_id: String,
    routing_table: HashMap<String, Vec<String>>, // repo_hash -> [node_ids]
}

impl DHT {
    pub fn new(node_id: String) -> Self {
        Self {
            node_id,
            routing_table: HashMap::new(),
        }
    }
    
    /// Announce that this node hosts a repository
    pub fn announce_content(&mut self, repo_hash: &str, node_id: &str) {
        self.routing_table
            .entry(repo_hash.to_string())
            .or_insert_with(Vec::new)
            .push(node_id.to_string());
    }
    
    /// Query which nodes host a repository
    pub fn query_content(&self, repo_hash: &str) -> Vec<String> {
        self.routing_table
            .get(repo_hash)
            .cloned()
            .unwrap_or_default()
    }
    
    /// Remove announcement
    pub fn unannounce_content(&mut self, repo_hash: &str, node_id: &str) {
        if let Some(nodes) = self.routing_table.get_mut(repo_hash) {
            nodes.retain(|n| n != node_id);
        }
    }
}

/// Periodically announce hosted repos to the DHT
pub async fn announcement_loop(state: crate::NodeState) {
    use tokio::time::{interval, Duration};
    
    let mut interval = interval(Duration::from_secs(300)); // Every 5 minutes
    
    loop {
        interval.tick().await;
        
        let repos = state.hosted_repos.read().await.clone();
        
        if let Some(dht) = state.dht.write().await.as_mut() {
            for repo_hash in repos {
                dht.announce_content(&repo_hash, &state.config.node_id);
                tracing::debug!("Announced {} to DHT", &repo_hash[..8]);
            }
        }
    }
}
