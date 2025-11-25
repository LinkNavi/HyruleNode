// ============================================================================
// Node/src/config.rs - Configuration Management
// ============================================================================

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Unique node identifier (hex string)
    pub node_id: String,
    
    /// Ed25519 public key (hex encoded)
    pub public_key: String,
    
    /// Ed25519 private key (hex encoded) - Keep this secure!
    pub private_key: String,
    
    /// Hyrule server address (defaults to onion address)
    pub hyrule_server: String,
    
    /// Port to listen on
    pub port: u16,
    
    /// Storage path for repositories
    pub storage_path: String,
    
    /// Storage capacity in bytes
    pub storage_capacity: u64,
    
    /// Whether this is an anchor node
    pub is_anchor: bool,
    
    /// Maximum bandwidth in Mbps
    pub max_bandwidth_mbps: u32,
    
    /// Enable Tor proxy for all connections
    pub enable_proxy: bool,
    
    /// SOCKS5 proxy address (Tor) - NOT optional
    pub proxy_addr: String,
    
    /// Enable onion routing
    pub enable_onion_routing: bool,
    
    /// Enable DHT for content discovery
    pub enable_dht: bool,
    
    /// Automatically replicate unhealthy repositories
    pub auto_replicate: bool,
    
    /// Maximum concurrent uploads
    pub max_concurrent_uploads: u32,
    
    /// Maximum concurrent downloads
    pub max_concurrent_downloads: u32,
}

impl NodeConfig {
    /// Generate a new node configuration with cryptographic identity
    pub fn generate() -> Self {
        use ed25519_dalek::SigningKey;
        use rand::rngs::OsRng;
        
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();
        
        let private_key_hex = hex::encode(signing_key.to_bytes());
        let public_key_hex = hex::encode(verifying_key.to_bytes());
        
        // Generate node ID as hex string from public key hash
        let node_id = hex::encode(blake3::hash(verifying_key.as_bytes()).as_bytes());
        
        Self {
            node_id,
            public_key: public_key_hex,
            private_key: private_key_hex,
            hyrule_server: "http://hyrule4e3tu7pfdkvvca43senvgvgisi6einpe3d3kpidlk3uyjf7lqd.onion".to_string(),
            port: 8080,
            storage_path: "node-storage".to_string(),
            storage_capacity: 10 * 1024 * 1024 * 1024, // 10 GB
            is_anchor: false,
            max_bandwidth_mbps: 100,
            enable_proxy: true,
            proxy_addr: "127.0.0.1:9050".to_string(),
            enable_onion_routing: true,
            enable_dht: true,
            auto_replicate: true,
            max_concurrent_uploads: 5,
            max_concurrent_downloads: 10,
        }
    }
    
    /// Get the config file path - checks current directory first
    pub fn config_path() -> Result<PathBuf> {
        // Priority 1: Check current directory for hyrule-node.toml
        let local_config = PathBuf::from("hyrule-node.toml");
        if local_config.exists() {
            return Ok(local_config);
        }
        
        // Priority 2: Check for .hyrule-node.toml (hidden)
        let hidden_config = PathBuf::from(".hyrule-node.toml");
        if hidden_config.exists() {
            return Ok(hidden_config);
        }
        
        // Priority 3: Fall back to system config directory
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;
        
        let hyrule_dir = config_dir.join("hyrule-node");
        std::fs::create_dir_all(&hyrule_dir)?;
        
        Ok(hyrule_dir.join("config.toml"))
    }
    
    /// Load configuration from file WITHOUT applying defaults
    /// This prevents overwriting user values
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        
        if !path.exists() {
            anyhow::bail!(
                "Config file not found at {}. Run 'hyrule-node init' first.",
                path.display()
            );
        }
        
        let content = std::fs::read_to_string(&path)?;
        let config: Self = toml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse config: {}", e))?;
        
        tracing::debug!("Loaded config from: {}", path.display());
        
        Ok(config)
    }
    
    /// Load config or create a new one if it doesn't exist
    pub fn load_or_create() -> Result<Self> {
        match Self::load() {
            Ok(config) => Ok(config),
            Err(_) => {
                tracing::info!("No config found, generating new one...");
                let config = Self::generate();
                config.save()?;
                Ok(config)
            }
        }
    }
    
    /// Save configuration to file - preserves ALL fields exactly as they are
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        
        tracing::debug!("Configuration saved to {}", path.display());
        
        Ok(())
    }
    
    /// Update specific fields and save - ONLY updates provided values
    pub fn update_and_save(
        &mut self,
        server: Option<String>,
        port: Option<u16>,
        storage_path: Option<String>,
        capacity_gb: Option<u64>,
        is_anchor: Option<bool>,
        enable_proxy: Option<bool>,
        proxy_addr: Option<String>,
        enable_dht: Option<bool>,
    ) -> Result<bool> {
        let mut changed = false;
        
        if let Some(srv) = server {
            if self.hyrule_server != srv {
                self.hyrule_server = srv;
                changed = true;
            }
        }
        
        if let Some(p) = port {
            if self.port != p {
                self.port = p;
                changed = true;
            }
        }
        
        if let Some(path) = storage_path {
            if self.storage_path != path {
                self.storage_path = path;
                changed = true;
            }
        }
        
        if let Some(cap) = capacity_gb {
            let cap_bytes = cap * 1024 * 1024 * 1024;
            if self.storage_capacity != cap_bytes {
                self.storage_capacity = cap_bytes;
                changed = true;
            }
        }
        
        if let Some(anchor) = is_anchor {
            if self.is_anchor != anchor {
                self.is_anchor = anchor;
                changed = true;
            }
        }
        
        if let Some(proxy) = enable_proxy {
            if self.enable_proxy != proxy {
                self.enable_proxy = proxy;
                self.enable_onion_routing = proxy;
                changed = true;
            }
        }
        
        if let Some(addr) = proxy_addr {
            if self.proxy_addr != addr {
                self.proxy_addr = addr;
                changed = true;
            }
        }
        
        if let Some(dht) = enable_dht {
            if self.enable_dht != dht {
                self.enable_dht = dht;
                changed = true;
            }
        }
        
        if changed {
            self.save()?;
        }
        
        Ok(changed)
    }
    
    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        // Validate port
        if self.port == 0 {
            anyhow::bail!("Invalid port number");
        }
        
        // Validate storage capacity
        if self.storage_capacity == 0 {
            anyhow::bail!("Storage capacity must be greater than 0");
        }
        
        // Validate public key format
        if hex::decode(&self.public_key).is_err() {
            anyhow::bail!("Invalid public key format");
        }
        
        // Validate private key
        if hex::decode(&self.private_key).is_err() {
            anyhow::bail!("Invalid private key format");
        }
        
        // Validate Tor settings
        if self.enable_proxy && self.proxy_addr.is_empty() {
            anyhow::bail!("Proxy enabled but no proxy address configured");
        }
        
        Ok(())
    }
    
    /// Get storage capacity in human-readable format
    pub fn storage_capacity_gb(&self) -> f64 {
        self.storage_capacity as f64 / (1024.0 * 1024.0 * 1024.0)
    }
    
    /// Check if Tor is properly configured
    pub fn is_tor_enabled(&self) -> bool {
        self.enable_proxy && !self.proxy_addr.is_empty()
    }
    
    /// Get the proxy address
    pub fn get_proxy_addr(&self) -> String {
        self.proxy_addr.clone()
    }
    
    /// Check if using onion service
    pub fn is_using_onion(&self) -> bool {
        self.hyrule_server.contains(".onion")
    }
    
    /// Get public address for registration (returns node_id based address)
    pub fn public_address(&self) -> String {
        // For Tor nodes, we use the node_id as the identifier
        // The actual .onion address would be configured separately
        format!("{}.local", &self.node_id[..16])
    }
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self::generate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_generate_config() {
        let config = NodeConfig::generate();
        assert!(!config.node_id.is_empty());
        assert!(!config.public_key.is_empty());
        assert!(!config.private_key.is_empty());
        assert_eq!(config.port, 8080);
        assert!(config.enable_proxy);
        assert!(config.enable_dht);
    }
    
    #[test]
    fn test_default_hyrule_server() {
        let config = NodeConfig::generate();
        assert!(config.hyrule_server.contains(".onion"));
        assert!(config.is_using_onion());
    }
    
    #[test]
    fn test_validate_config() {
        let config = NodeConfig::generate();
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_is_tor_enabled() {
        let config = NodeConfig::generate();
        assert!(config.is_tor_enabled());
    }
}
