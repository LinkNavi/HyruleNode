use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use ed25519_dalek::SigningKey;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub node_id: String,
    pub public_key: String,
    pub private_key: String,
    pub hyrule_server: String,
pub public_address: String,

    pub port: u16,
    pub storage_path: String,
    pub storage_capacity: u64,
    pub is_anchor: bool,
    pub max_bandwidth_mbps: u32,
    pub enable_proxy: bool,
    pub proxy_addr: Option<String>,
    pub enable_onion_routing: bool,
    pub enable_dht: bool,
    pub auto_replicate: bool,
    pub max_concurrent_uploads: usize,
    pub max_concurrent_downloads: usize,
}

impl NodeConfig {
    pub fn generate() -> Self {
        use rand::rngs::OsRng;
        
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();
        
        let private_key_bytes = signing_key.to_bytes();
        let public_key_bytes = verifying_key.to_bytes();
        
        let node_id = hex::encode(blake3::hash(&public_key_bytes).as_bytes());
        
        Self {
            node_id,
            public_key: hex::encode(public_key_bytes),
            private_key: hex::encode(private_key_bytes),
            // Default to onion address
            hyrule_server: "http://hyrule4e3tu7pfdkvvca43senvgvgisi6einpe3d3kpidlk3uyjf7lqd.onion".to_string(),
            port: 8080,
public_address: "hyrule4e3tu7pfdkvvca43senvgvgisi6einpe3d3kpidlk3uyjf7lqd.onion".to_string(),

            storage_path: "node-storage".to_string(),
            storage_capacity: 10 * 1024 * 1024 * 1024, // 10 GB
            is_anchor: false,
            max_bandwidth_mbps: 100,
            // Enable proxy by default for Tor
            enable_proxy: true,
            proxy_addr: Some("127.0.0.1:9050".to_string()),
            enable_onion_routing: true,
            enable_dht: true,
            auto_replicate: true,
            max_concurrent_uploads: 5,
            max_concurrent_downloads: 10,
        }
    }
    
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::config_path()?;
        
        if !path.exists() {
            anyhow::bail!("Config not found. Run 'hyrule-node init' first.");
        }
        
        let contents = fs::read_to_string(&path)?;
        let config: NodeConfig = toml::from_str(&contents)?;
        Ok(config)
    }
    
    pub fn load_or_create() -> anyhow::Result<Self> {
        match Self::load() {
            Ok(config) => Ok(config),
            Err(_) => {
                let config = Self::generate();
                config.save()?;
                Ok(config)
            }
        }
    }
    
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path()?;
        
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let contents = toml::to_string_pretty(self)?;
        fs::write(&path, contents)?;
        Ok(())
    }
    
    pub fn config_path() -> anyhow::Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;
        Ok(config_dir.join("hyrule-node").join("config.toml"))
    }
}
