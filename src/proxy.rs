// src/proxy.rs

use arti_client::{TorClient, TorClientConfig};
use arti_hyper::ArtiHttpConnector;
use tor_rtcompat::tokio::TokioNativeTlsRuntime;
use tls_api::{TlsConnector as TlsConnectorTrait, TlsConnectorBuilder}; // Added Builder trait
use tls_api_native_tls::TlsConnector;
use anyhow::Result;
use std::sync::Arc;
use hyper::{Client as HyperClient, Body};

// Import our new wrapper
use crate::http_client::HyruleClient;

// We keep the raw type alias for internal use if needed
type InnerHttpClient = HyperClient<ArtiHttpConnector<TokioNativeTlsRuntime, TlsConnector>, Body>;

#[derive(Clone)]
pub struct ProxyConfig {
    pub enabled: bool,
    pub addr: String,
    tor_client: Option<Arc<TorClient<TokioNativeTlsRuntime>>>,
}

impl ProxyConfig {
    pub fn from_config(config: &crate::config::NodeConfig) -> Self {
        Self {
            enabled: config.enable_proxy,
            addr: if config.proxy_addr.is_empty() {
                "127.0.0.1:9050".to_string()
            } else {
                config.proxy_addr.clone()
            },
            tor_client: None,
        }
    }
    
pub async fn init_tor_client(&mut self) -> Result<()> {
    if !self.enabled {
        return Ok(());
    }
    tracing::info!("🧅 Bootstrapping Arti Tor client...");
    
    let config = TorClientConfig::default();
    let runtime = TokioNativeTlsRuntime::current()?;
    let tor_client = TorClient::with_runtime(runtime)
        .config(config)
        .create_bootstrapped()
        .await?;
    tracing::info!("✓ Arti Tor client bootstrapped successfully");
    self.tor_client = Some(Arc::new(tor_client));
    Ok(())
}
    pub fn get_tor_client(&self) -> Option<Arc<TorClient<TokioNativeTlsRuntime>>> {
        self.tor_client.clone()
    }
    
    // CHANGED: Return HyruleClient instead of generic Hyper Client

pub fn build_client(&self) -> Result<HyruleClient> {
    if !self.enabled {
        anyhow::bail!("Tor is disabled in config");
    }
    
    if self.tor_client.is_none() {
        anyhow::bail!("Tor client not initialized - call init_tor_client() first");
    }

    tracing::debug!("Building client with initialized Tor");
    
    // deref Arc and clone to get TorClient
    let tor_client = (**self.tor_client.as_ref().unwrap()).clone();

    // Build TLS connector
    let tls_conn = <TlsConnector as TlsConnectorTrait>::builder()?.build()?;

    // Create connector
    let connector = ArtiHttpConnector::new(tor_client, tls_conn);

    // Build Hyper client
    let inner_client = HyperClient::builder().build(connector);

    Ok(HyruleClient::new(inner_client))
}
    
    pub fn build_tor_client(&self) -> Result<HyruleClient> {
        self.build_client()
    }

pub async fn validate_tor_connection(&self) -> Result<()> {
    if !self.enabled || self.tor_client.is_none() {
        anyhow::bail!("Tor is not enabled");
    }
    let tor_client = self.tor_client.as_ref().unwrap();
    
    // Create stream preferences that allow onion addresses
    let mut prefs = arti_client::StreamPrefs::new();
    prefs.connect_to_onion_services(arti_client::config::BoolOrAuto::Explicit(true));
    
    let test_addr = ("hyrule4e3tu7pfdkvvca43senvgvgisi6einpe3d3kpidlk3uyjf7lqd.onion", 80);
    
    // Increase timeout to 60 seconds for initial connection
    match tokio::time::timeout(
        std::time::Duration::from_secs(60), 
        tor_client.connect_with_prefs(test_addr, &prefs)
    ).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(e)) => anyhow::bail!("Tor connection failed: {}", e),
        Err(_) => anyhow::bail!("Tor connection timed out after 60s"),
    }
}

}
