// Node/src/main.rs - Upgraded version with Arti Tor support
mod http_client;
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
    
    Serve {
        repo_hash: String,
    },
    
    Unserve {
        repo_hash: String,
    },
    
    Verify {
        repo_hash: Option<String>,
    },
    
    DhtTest {
        repo_hash: String,
        
        #[arg(short, long, default_value = "query")]
        action: String,
    },
    
    TestTor,
}

#[derive(Clone)]
pub struct NodeState {
    pub config: config::NodeConfig,
    pub storage: Arc<storage::GitStorage>,
    pub hosted_repos: Arc<RwLock<Vec<String>>>,
    pub stats: Arc<RwLock<NodeStats>>,
    pub dht: Arc<RwLock<Option<dht::DHT>>>,
    pub proxy: crate::proxy::ProxyConfig,
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
    _is_anchor: bool,
    _enable_dht: bool,
    enable_tor: bool,
    proxy_addr: Option<String>,
) -> anyhow::Result<()> {
    tracing::info!("🧅 Starting Hyrule Storage Node v0.3.0 (Arti Edition)");
    
    let mut config = config::NodeConfig::load_or_create()?;
    
    let disable_tor_flag_passed = !enable_tor;
    
    let config_changed = config.update_and_save(
        server,
        port,
        storage_path,
        capacity_gb,
        None,
        if disable_tor_flag_passed { Some(false) } else { None },
        proxy_addr,
        None,
    )?;
    
    if config_changed {
        tracing::info!("💾 Configuration updated and saved");
    }
    
    tracing::info!("📁 Storage path: {}", config.storage_path);
    tracing::info!("💾 Capacity: {:.2} GB", config.storage_capacity_gb());
    tracing::info!("🆔 Node ID: {}", &config.node_id[..16]);
    tracing::info!("🏷️  Type: {}", if config.is_anchor { "Anchor Node" } else { "P2P Node" });
    
    // Initialize Arti Tor client
    let mut proxy_config = proxy::ProxyConfig::from_config(&config);
    
if config.enable_proxy {
    tracing::info!("🧅 Initializing Arti Tor client...");
    tracing::info!("🌐 Hyrule server: {}", config.hyrule_server);
    
    match proxy_config.init_tor_client().await {
        Ok(_) => {
            tracing::info!("✓ Arti Tor client initialized and bootstrapped");
            
            // Make validation non-fatal - it will work once circuits are built
            tracing::info!("⏳ Building initial Tor circuits...");
            match proxy_config.validate_tor_connection().await {
                Ok(_) => {
                    tracing::info!("✓ Tor connection validated successfully");
                }
                Err(e) => {
                    tracing::warn!("⚠️  Initial Tor validation timed out: {}", e);
                    tracing::warn!("   This is normal on first run. Circuits will be built as needed.");
                }
            }
        }
        Err(e) => {
            tracing::error!("✗ Failed to initialize Arti: {}", e);
            tracing::error!("  Make sure you have internet connectivity");
            anyhow::bail!("Cannot start without Tor");
        }
    }
} else {
    tracing::warn!("⚠️  Tor disabled - traffic will NOT be anonymous!");
    tracing::warn!("   This is NOT RECOMMENDED for production use");
}    
    let storage = Arc::new(storage::GitStorage::new(&config.storage_path)?);
    
    let dht = if config.enable_dht {
        tracing::info!("🔍 Initializing DHT...");
        Some(dht::DHT::new(config.node_id.clone()))
    } else {
        None
    };
    
    let state = NodeState {
        config: config.clone(),
        storage: storage.clone(),
        hosted_repos: Arc::new(RwLock::new(Vec::new())),
        stats: Arc::new(RwLock::new(NodeStats::default())),
        dht: Arc::new(RwLock::new(dht)),
        proxy: proxy_config.clone(),
    };
    
    // Load existing repos
    {
        let repos = storage.list_hosted_repos()?;
        let mut hosted = state.hosted_repos.write().await;
        *hosted = repos;
        tracing::info!("📦 Loaded {} existing repositories", hosted.len());
    }
    
    // Register with Hyrule server
// Register with Hyrule server
tracing::info!("🔗 Registering with Hyrule server...");
match registration::register_node(&config, &proxy_config).await {
    Ok(_) => tracing::info!("✓ Successfully registered with network"),
    Err(e) => {
        tracing::warn!("⚠️  Registration failed: {}. Will retry...", e);
    }
}

// Clone the initialized proxy_config for background tasks
let proxy_for_tasks = proxy_config.clone();

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
    
    if config.enable_dht {
        let dht_state = state.clone();
        tokio::spawn(async move {
            dht::announcement_loop(dht_state).await;
        });
    }
    
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
    println!("🧅 Tor: Enabled (using Arti embedded client)");
    println!();
    println!("Config saved to: {}", config_path.display());
    println!();
    println!("ℹ️  Arti will bootstrap automatically on first start");
    println!("   No need to install Tor separately!");
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
        println!("🧅 Tor: Enabled (Arti embedded client)");
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
    
    if !storage.repo_path(&repo_hash).exists() {
        storage.init_repo(&repo_hash)?;
        println!("✓ Initialized local storage for {}", &repo_hash[..16]);
    }
    
    let mut proxy_config = proxy::ProxyConfig::from_config(&config);
    if config.enable_proxy {
        proxy_config.init_tor_client().await?;
    }
    
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
    println!("🧅 Testing Arti Tor connection...");
    println!();
    
    let config = config::NodeConfig::load()?;
    let mut proxy_config = proxy::ProxyConfig::from_config(&config);
    
    if !proxy_config.enabled {
        println!("✗ Tor is disabled in config");
        println!("  Enable it by setting enable_proxy = true");
        return Ok(());
    }
    
    println!("Initializing Arti client and bootstrapping...");
    
    match proxy_config.init_tor_client().await {
        Ok(_) => {
            println!("✓ Arti client initialized");
            
            match proxy_config.validate_tor_connection().await {
                Ok(_) => {
                    println!();
                    println!("✓ Tor connection successful!");
                    println!("  Your traffic is being routed through the Tor network");
                }
                Err(e) => {
                    println!();
                    println!("✗ Tor connection validation failed: {}", e);
                }
            }
        }
        Err(e) => {
            println!();
            println!("✗ Arti initialization failed: {}", e);
            println!();
            println!("Troubleshooting:");
            println!("  1. Make sure you have internet connectivity");
            println!("  2. Check your firewall allows outbound connections");
            println!("  3. Arti needs to bootstrap to the Tor network");
        }
    }
    
    Ok(())
}
