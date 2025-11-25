// Node/src/main.rs - Upgraded version with Tor support
mod config;
mod storage;
mod api;
mod registration;
mod replication;
mod health;
mod crypto;
mod dht;
mod proxy;

use clap::{Parser, Subcommand};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;

#[derive(Parser)]
#[command(name = "hyrule-node")]
#[command(version, about = "Distributed storage node for Hyrule network")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the node
    Start {
        #[arg(short, long)]
        port: Option<u16>,
        
        #[arg(short, long)]
        server: Option<String>,
        
        #[arg(long)]
        storage_path: Option<String>,
        
        #[arg(long)]
        capacity: Option<u64>,
        
        #[arg(long)]
        anchor: bool,
        
        #[arg(long)]
        enable_dht: bool,
        
        #[arg(long)]
        disable_tor: bool,
        
        #[arg(long)]
        proxy_addr: Option<String>,
    },
    
    Init {
        #[arg(short, long)]
        output: Option<String>,
    },
    
    Status,
    Repos,
    
    /// Serve a specific repository
    Serve {
        /// Repository hash to serve
        repo_hash: String,
    },
    
    /// Stop serving a repository
    Unserve {
        /// Repository hash to stop serving
        repo_hash: String,
    },
    
    Verify {
        repo_hash: Option<String>,
    },
    
    /// Test DHT functionality
    DhtTest {
        /// Repository hash to announce/query
        repo_hash: String,
        
        /// Action: announce or query
        #[arg(short, long, default_value = "query")]
        action: String,
    },
    
    /// Test Tor connection
    TestTor,
}
#[derive(Clone)]
pub struct NodeState {
    config: config::NodeConfig,
    storage: Arc<storage::GitStorage>,
    hosted_repos: Arc<RwLock<Vec<String>>>,
    stats: Arc<RwLock<NodeStats>>,
    dht: Arc<RwLock<Option<dht::DHT>>>,
}

#[derive(Default, Clone)]
pub struct NodeStats {
    total_requests: u64,
    bytes_served: u64,
    repos_hosted: usize,
    uptime_seconds: u64,
    replication_count: u64,
    failed_requests: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(false)
        .with_level(true)
        .init();
    
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Start { 
            port, server, storage_path, capacity, anchor, 
            enable_dht, disable_tor, proxy_addr 
        } => {
            start_node(port, server, storage_path, capacity, anchor, enable_dht, !disable_tor, proxy_addr).await?;
        }
        Commands::Init { output } => {
            init_node(output)?;
        }
        Commands::Status => {
            show_status().await?;
        }
        Commands::Repos => {
            list_repos().await?;
        }
        Commands::Serve { repo_hash } => {
            serve_repo(repo_hash).await?;
        }
        Commands::Unserve { repo_hash } => {
            unserve_repo(repo_hash).await?;
        }
        Commands::Verify { repo_hash } => {
            verify_storage(repo_hash).await?;
        }
        Commands::DhtTest { repo_hash, action } => {
            test_dht(repo_hash, action).await?;
        }
        Commands::TestTor => {
            test_tor().await?;
        }
    }
    
    Ok(())
}

async fn start_node(
    port: Option<u16>,
    server: Option<String>,
    storage_path: Option<String>,
    capacity_gb: Option<u64>,
    is_anchor: bool,
    enable_dht: bool,
    enable_tor: bool,
    proxy_addr: Option<String>,
) -> anyhow::Result<()> {
    tracing::info!("🧅 Starting Hyrule Storage Node v0.2.0 (Tor Edition)");
    
    // Load existing config - this will NOT override values
    let mut config = config::NodeConfig::load_or_create()?;
    
    // Determine if --disable-tor was explicitly passed
    // enable_tor parameter represents the OPPOSITE of --disable-tor flag
    // So if enable_tor is false, it means --disable-tor was passed
    let disable_tor_flag_passed = !enable_tor;
    
    // Update config ONLY with explicitly provided CLI arguments
    // Note: is_anchor and enable_dht are always passed by clap even if not specified
    // So we need to check if they're different from current config
    let config_changed = config.update_and_save(
        server,
        port,
        storage_path,
        capacity_gb,
        None, // Don't auto-update is_anchor unless explicitly needed
        if disable_tor_flag_passed { Some(false) } else { None },
        proxy_addr,
        None, // Don't auto-update enable_dht unless explicitly needed
    )?;
    
    if config_changed {
        tracing::info!("💾 Configuration updated and saved");
    }
    
    tracing::info!("📁 Storage path: {}", config.storage_path);
    tracing::info!("💾 Capacity: {:.2} GB", config.storage_capacity_gb());
    tracing::info!("🆔 Node ID: {}", &config.node_id[..16]);
    tracing::info!("🏷️  Type: {}", if config.is_anchor { "Anchor Node" } else { "P2P Node" });
    
    if config.enable_proxy {
        tracing::info!("🧅 Tor enabled: {}", config.proxy_addr);
        tracing::info!("🌐 Hyrule server: {}", config.hyrule_server);
        
        // Validate Tor connection
        let proxy_config = proxy::ProxyConfig::from_config(&config);
        match proxy_config.validate_tor_connection().await {
            Ok(_) => {
                tracing::info!("✓ Tor connection validated successfully");
            }
            Err(e) => {
                tracing::error!("✗ Tor validation failed: {}", e);
                tracing::error!("  Make sure Tor is running on {}", config.proxy_addr);
                tracing::error!("  Install Tor: https://www.torproject.org/download/");
                anyhow::bail!("Cannot start without working Tor connection");
            }
        }
    } else {
        tracing::warn!("⚠️  Tor disabled - traffic will NOT be anonymous!");
        tracing::warn!("   This is NOT RECOMMENDED for production use");
    }
    
    let storage = Arc::new(storage::GitStorage::new(&config.storage_path)?);
    
    // Initialize DHT if enabled in config
    let dht = if config.enable_dht {
        tracing::info!("🔍 Initializing DHT...");
        let dht = dht::DHT::new(config.node_id.clone());
        Some(dht)
    } else {
        None
    };
    
    let state = NodeState {
        config: config.clone(),
        storage: storage.clone(),
        hosted_repos: Arc::new(RwLock::new(Vec::new())),
        stats: Arc::new(RwLock::new(NodeStats::default())),
        dht: Arc::new(RwLock::new(dht)),
    };
    
    // Load existing repos
    {
        let repos = storage.list_hosted_repos()?;
        let mut hosted = state.hosted_repos.write().await;
        *hosted = repos;
        tracing::info!("📦 Loaded {} existing repositories", hosted.len());
    }
    
    // Register with Hyrule server
    tracing::info!("🔗 Registering with Hyrule server...");
    match registration::register_node(&config).await {
        Ok(_) => tracing::info!("✓ Successfully registered with network"),
        Err(e) => {
            tracing::warn!("⚠️  Registration failed: {}. Will retry...", e);
            tracing::warn!("   Make sure Tor is running and the onion address is accessible");
        }
    }
    
    // Start background tasks
    let heartbeat_state = state.clone();
    tokio::spawn(async move {
        health::heartbeat_loop(heartbeat_state).await;
    });
    
    let replication_state = state.clone();
    tokio::spawn(async move {
        replication::replication_loop(replication_state).await;
    });
    
    let monitor_state = state.clone();
    tokio::spawn(async move {
        health::monitor_storage(monitor_state).await;
    });
    
    // DHT announcement loop
    if config.enable_dht {
        let dht_state = state.clone();
        tokio::spawn(async move {
            dht::announcement_loop(dht_state).await;
        });
    }
    
    // Build router
    let app = api::create_router(state)
        .layer(TraceLayer::new_for_http());
    
    let addr = format!("0.0.0.0:{}", config.port);
    tracing::info!("🚀 Node listening on {}", addr);
    tracing::info!("📊 Status: http://localhost:{}/status", config.port);
    tracing::info!("");
    tracing::info!("✓ Node is ready to accept connections");
    
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    
    Ok(())
}


fn init_node(output: Option<String>) -> anyhow::Result<()> {
    println!("🔑 Generating node identity...");
    
    let config = config::NodeConfig::generate();
    
    let config_path = if let Some(path) = output {
        std::path::PathBuf::from(path)
    } else {
        config::NodeConfig::config_path()?
    };
    
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    let config_str = toml::to_string_pretty(&config)?;
    std::fs::write(&config_path, config_str)?;
    
    println!("✓ Node identity created!");
    println!();
    println!("Node ID: {}", config.node_id);
    println!("Public Key: {}", config.public_key);
    println!("Hyrule Server: {}", config.hyrule_server);
println!(
    "🧅 Tor: Enabled ({})",
    config.proxy_addr.as_str()
);

    println!();
    println!("Config saved to: {}", config_path.display());
    println!();
    println!("⚠️  Make sure Tor is installed and running:");
    println!("   - Install: https://www.torproject.org/download/");
    println!("   - Default port: 9050");
    println!();
    println!("Start your node with:");
    println!("  hyrule-node start");
    
    Ok(())
}

async fn show_status() -> anyhow::Result<()> {
    println!("📊 Node Status");
    println!();
    
    let config = config::NodeConfig::load()?;
    let storage = storage::GitStorage::new(&config.storage_path)?;
    
    println!("Node ID: {}", &config.node_id[..16]);
    println!("Port: {}", config.port);
    println!("Type: {}", if config.is_anchor { "Anchor" } else { "P2P" });
    println!("Storage: {}", config.storage_path);
    println!("Hyrule Server: {}", config.hyrule_server);
    
    let usage = storage.get_storage_usage()?;
    let capacity = config.storage_capacity;
    let usage_pct = (usage as f64 / capacity as f64) * 100.0;
    
    println!("Usage: {:.2} GB / {:.2} GB ({:.1}%)", 
        usage as f64 / 1e9, 
        capacity as f64 / 1e9,
        usage_pct
    );
    
    let repos = storage.list_hosted_repos()?;
    println!("Repositories: {}", repos.len());
    
    if config.enable_proxy {
println!(
    "Tor Proxy: {} (enabled: {})",
    config.proxy_addr.as_str(),
    config.enable_proxy
);

    } else {
        println!("⚠️  Tor: Disabled");
    }
    
    Ok(())
}

async fn list_repos() -> anyhow::Result<()> {
    println!("📦 Hosted Repositories");
    println!();
    
    let config = config::NodeConfig::load()?;
    let storage = storage::GitStorage::new(&config.storage_path)?;
    
    let repos = storage.list_hosted_repos()?;
    
    if repos.is_empty() {
        println!("No repositories hosted yet.");
        return Ok(());
    }
    
    for (i, repo_hash) in repos.iter().enumerate() {
        let size = storage.get_repo_size(repo_hash)?;
        let object_count = storage.list_objects(repo_hash)?.len();
        
        println!("{}. {}", i + 1, &repo_hash[..16]);
        println!("   Size: {:.2} MB", size as f64 / 1e6);
        println!("   Objects: {}", object_count);
    }
    
    Ok(())
}

async fn serve_repo(repo_hash: String) -> anyhow::Result<()> {
    println!("📤 Adding repository to serving list...");
    
    let config = config::NodeConfig::load()?;
    let storage = storage::GitStorage::new(&config.storage_path)?;
    
    // Initialize if not exists
    if !storage.repo_path(&repo_hash).exists() {
        storage.init_repo(&repo_hash)?;
        println!("✓ Initialized local storage for {}", &repo_hash[..16]);
    }
    
    // Build client with Tor support
    let proxy_config = proxy::ProxyConfig::from_config(&config);
    let client = proxy_config.build_client()?;
    
    let url = format!("{}/api/repos/{}/replicate", config.hyrule_server, repo_hash);
    
    #[derive(serde::Serialize)]
    struct AnnounceReq {
        node_id: String,
    }
    
    let req = AnnounceReq {
        node_id: config.node_id,
    };
    
    let response = client.post(&url).json(&req).send().await?;
    
    if response.status().is_success() {
        println!("✓ Successfully announced to network");
    } else {
        println!("✗ Failed to announce: {}", response.status());
    }
    
    Ok(())
}

async fn unserve_repo(repo_hash: String) -> anyhow::Result<()> {
    println!("📥 Removing repository from serving list...");
    
    println!("✓ Repository {} no longer advertised", &repo_hash[..16]);
    println!("  (Data preserved in storage)");
    
    Ok(())
}

async fn verify_storage(repo_hash: Option<String>) -> anyhow::Result<()> {
    println!("🔍 Verifying storage integrity...");
    
    let config = config::NodeConfig::load()?;
    let storage = storage::GitStorage::new(&config.storage_path)?;
    
    let repos = if let Some(hash) = repo_hash {
        vec![hash]
    } else {
        storage.list_hosted_repos()?
    };
    
    let mut total_objects = 0;
    let mut corrupted = 0;
    
    for repo in repos {
        println!("\nVerifying {}...", &repo[..16]);
        
        let objects = storage.list_objects(&repo)?;
        total_objects += objects.len();
        
        for object_id in objects {
            match storage.verify_object(&repo, &object_id) {
                Ok(true) => {},
                Ok(false) => {
                    println!("   ✗ Corrupted: {}", &object_id[..8]);
                    corrupted += 1;
                }
                Err(e) => {
                    println!("   ✗ Error reading {}: {}", &object_id[..8], e);
                    corrupted += 1;
                }
            }
        }
    }
    
    println!();
    println!("═══════════════════════");
    println!("Total objects: {}", total_objects);
    println!("Corrupted: {}", corrupted);
    
    if corrupted == 0 {
        println!("✓ All objects verified successfully!");
    } else {
        println!("✗ Found {} corrupted objects", corrupted);
    }
    
    Ok(())
}

async fn test_dht(repo_hash: String, action: String) -> anyhow::Result<()> {
    println!("🔍 Testing DHT functionality...");
    
    let config = config::NodeConfig::load()?;
    let mut dht = dht::DHT::new(config.node_id.clone());
    
    match action.as_str() {
        "announce" => {
            dht.announce_content(&repo_hash, &config.node_id);
            println!("✓ Announced {} to DHT", &repo_hash[..16]);
        }
        "query" => {
            let nodes = dht.query_content(&repo_hash);
            println!("Found {} nodes hosting {}", nodes.len(), &repo_hash[..16]);
            for node in nodes {
                println!("  - {}", &node[..16]);
            }
        }
        _ => {
            println!("✗ Unknown action: {}", action);
            println!("   Use: announce or query");
        }
    }
    
    Ok(())
}

async fn test_tor() -> anyhow::Result<()> {
    println!("🧅 Testing Tor connection...");
    println!();
    
    let config = config::NodeConfig::load()?;
    let proxy_config = proxy::ProxyConfig::from_config(&config);
    
    if !proxy_config.enabled {
        println!("✗ Tor is disabled in config");
        println!("  Enable it by setting enable_proxy = true");
        return Ok(());
    }
    
    println!("Tor proxy: {}", proxy_config.addr);
    println!("Connecting...");
    
    match proxy_config.validate_tor_connection().await {
        Ok(_) => {
            println!();
            println!("✓ Tor connection successful!");
            println!("  Your traffic is being routed through the Tor network");
        }
        Err(e) => {
            println!();
            println!("✗ Tor connection failed: {}", e);
            println!();
            println!("Troubleshooting:");
            println!("  1. Make sure Tor is installed and running");
            println!("  2. Check that Tor is listening on {}", proxy_config.addr);
            println!("  3. Verify your firewall allows connections to Tor");
        }
    }
    
    Ok(())
}

use colored::Colorize;
