k// ============================================================================
// Node/src/proxy.rs - Tor Proxy Support Module
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
    
    /// Create HTTP client with SOCKS5 proxy (Tor)
    /// This enforces all traffic goes through Tor when enabled
    pub fn build_client(&self) -> reqwest::Result<reqwest::Client> {
        let mut builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60)) // Longer timeout for Tor
            .danger_accept_invalid_certs(false); // Still validate certs
        
        if self.enabled {
            // Route through SOCKS5 proxy (Tor)
            let proxy = reqwest::Proxy::all(&format!("socks5h://{}", self.addr))?;
            builder = builder.proxy(proxy);
            
            tracing::debug!("HTTP client configured to use Tor at {}", self.addr);
        } else {
            tracing::warn!("Proxy disabled - traffic will NOT be routed through Tor!");
        }
        
        builder.build()
    }
    
    /// Build a client that REQUIRES Tor (fails if proxy not enabled)
    pub fn build_tor_client(&self) -> anyhow::Result<reqwest::Client> {
        if !self.enabled {
            anyhow::bail!("Tor proxy must be enabled for this operation");
        }
        
        let proxy = reqwest::Proxy::all(&format!("socks5h://{}", self.addr))?;
        
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .proxy(proxy)
            .danger_accept_invalid_certs(false)
            .build()?;
        
        Ok(client)
    }
    
    /// Validate that Tor is accessible
    pub async fn validate_tor_connection(&self) -> anyhow::Result<()> {
        if !self.enabled {
            anyhow::bail!("Tor proxy is not enabled");
        }
        
        let client = self.build_client()?;
        
        // Try to connect to check.torproject.org to verify Tor connection
        tracing::info!("Validating Tor connection...");
        
        let response = client
            .get("https://check.torproject.org/api/ip")
            .send()
            .await?;
        
        if response.status().is_success() {
            let body = response.text().await?;
            if body.contains("\"IsTor\":true") {
                tracing::info!("✓ Successfully connected through Tor network");
                Ok(())
            } else {
                anyhow::bail!("Connected but not through Tor - check your proxy settings")
            }
        } else {
            anyhow::bail!("Failed to validate Tor connection: {}", response.status())
        }
    }
}
