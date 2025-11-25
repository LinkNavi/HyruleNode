
// ============================================================================
// Node/src/proxy.rs - Proxy Support Module
// ============================================================================

pub struct ProxyConfig {
    pub enabled: bool,
    pub addr: String,
}

impl ProxyConfig {
    pub fn from_config(config: &crate::config::NodeConfig) -> Self {
        Self {
            enabled: config.enable_proxy,
            addr: config.proxy_addr.clone().unwrap_or_else(|| "127.0.0.1:9050".to_string()),
        }
    }
    
    /// Create HTTP client with proxy if enabled
    pub fn build_client(&self) -> reqwest::Result<reqwest::Client> {
        let mut builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30));
        
        if self.enabled {
            let proxy = reqwest::Proxy::all(&format!("socks5://{}", self.addr))?;
            builder = builder.proxy(proxy);
        }
        
        builder.build()
    }
}
