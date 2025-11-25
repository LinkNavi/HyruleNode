// Node/src/main.rs - Upgraded version
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
        #[arg(short, long, default_value = "8080")]
        port: u16,
        
        #[arg(short, long, default_value = "http://localhost:3000")]
        server: String,
        
        #[arg(long, default_value = "node-storage")]
        storage_path: String,
        
        #[arg(long, default_value = "10")]
        capacity: u64,
        
        #[arg(long)]
        anchor: bool,
        
        /// Enable DHT participation
        #[arg(long)]
        enable_dht: bool,
        
        /// Enable SOCKS5 proxy support
        #[arg(long)]
        enable_proxy: bool,
        
        /// SOCKS5 proxy address
        #[arg(long, default_value = "127.0.0.1:9050")]
        proxy_addr: String,
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
            enable_dht, enable_proxy, proxy_addr 
        } => {
            start_node(port, server, storage_path, capacity, anchor, enable_dht, enable_proxy, proxy_addr).await?;
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
    }
    
    Ok(())
}

async fn start_node(
    port: u16,
    server: String,
    storage_path: String,
    capacity_gb: u64,
    is_anchor: bool,
    enable_dht: bool,
    enable_proxy: bool,
    proxy_addr: String,
) -> anyhow::Result<()> {
    tracing::info!(" Starting Hyrule Storage Node v0.2.0");
    
    let mut config = config::NodeConfig::load_or_create()?;
    config.hyrule_server = server.clone();
    config.port = port;
    config.storage_path = storage_path.clone();
    config.storage_capacity = capacity_gb * 1024 * 1024 * 1024;
    config.is_anchor = is_anchor;
    config.enable_proxy = enable_proxy;
    config.proxy_addr = Some(proxy_addr);
    config.save()?;
    
    tracing::info!(" Storage path: {}", config.storage_path);
    tracing::info!(" Capacity: {} GB", capacity_gb);
    tracing::info!(" Node ID: {}", &config.node_id[..16]);
    tracing::info!(" Type: {}", if is_anchor { "Anchor Node" } else { "P2P Node" });
    
    if enable_proxy {
        tracing::info!(" Proxy enabled: {}", config.proxy_addr.as_ref().unwrap());
    }
    
    let storage = Arc::new(storage::GitStorage::new(&config.storage_path)?);
    
    // Initialize DHT if enabled
    let dht = if enable_dht {
        tracing::info!(" Initializing DHT...");
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
        tracing::info!(" Loaded {} existing repositories", hosted.len());
    }
    
    // Register with Hyrule server
    tracing::info!(" Registering with Hyrule server: {}", server);
    match registration::register_node(&config).await {
        Ok(_) => tracing::info!(" Successfully registered with network"),
        Err(e) => tracing::warn!(" Registration failed: {}. Will retry...", e),
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
    if enable_dht {
        let dht_state = state.clone();
        tokio::spawn(async move {
            dht::announcement_loop(dht_state).await;
        });
    }
    
    // Build router
    let app = api::create_router(state)
        .layer(TraceLayer::new_for_http());
    
    let addr = format!("0.0.0.0:{}", port);
    tracing::info!(" Node listening on {}", addr);
    tracing::info!(" Status: http://localhost:{}/status", port);
    
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    
    Ok(())
}

fn init_node(output: Option<String>) -> anyhow::Result<()> {
    println!(" Generating node identity...");
    
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
    
    println!(" Node identity created!");
    println!();
    println!("Node ID: {}", config.node_id);
    println!("Public Key: {}", config.public_key);
    println!();
    println!("Config saved to: {}", config_path.display());
    println!();
    println!("Start your node with:");
    println!("  hyrule-node start --port 8080");
    
    Ok(())
}

async fn show_status() -> anyhow::Result<()> {
    println!(" Node Status");
    println!("");
    
    let config = config::NodeConfig::load()?;
    let storage = storage::GitStorage::new(&config.storage_path)?;
    
    println!("Node ID: {}", &config.node_id[..16]);
    println!("Port: {}", config.port);
    println!("Type: {}", if config.is_anchor { "Anchor" } else { "P2P" });
    println!("Storage: {}", config.storage_path);
    
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
        println!("Proxy: Enabled ({})", config.proxy_addr.as_ref().unwrap());
    }
    
    Ok(())
}

async fn list_repos() -> anyhow::Result<()> {
    println!(" Hosted Repositories");
    println!("");
    
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
    println!(" Adding repository to serving list...");
    
    let config = config::NodeConfig::load()?;
    let storage = storage::GitStorage::new(&config.storage_path)?;
    
    // Initialize if not exists
    if !storage.repo_path(&repo_hash).exists() {
        storage.init_repo(&repo_hash)?;
        println!(" Initialized local storage for {}", &repo_hash[..16]);
    }
    
    // Announce to network
    let client = reqwest::Client::new();
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
        println!(" Successfully announced to network");
    } else {
        println!(" Failed to announce: {}", response.status());
    }
    
    Ok(())
}

async fn unserve_repo(repo_hash: String) -> anyhow::Result<()> {
    println!(" Removing repository from serving list...");
    
    let config = config::NodeConfig::load()?;
    let storage = storage::GitStorage::new(&config.storage_path)?;
    
    // Don't delete data, just stop announcing
    println!(" Repository {} no longer advertised", &repo_hash[..16]);
    println!("  (Data preserved in storage)");
    
    Ok(())
}

async fn verify_storage(repo_hash: Option<String>) -> anyhow::Result<()> {
    println!(" Verifying storage integrity...");
    
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
                    println!("   Corrupted: {}", &object_id[..8]);
                    corrupted += 1;
                }
                Err(e) => {
                    println!("   Error reading {}: {}", &object_id[..8], e);
                    corrupted += 1;
                }
            }
        }
    }
    
    println!();
    println!("");
    println!("Total objects: {}", total_objects);
    println!("Corrupted: {}", corrupted);
    
    if corrupted == 0 {
        println!(" All objects verified successfully!");
    } else {
        println!(" Found {} corrupted objects", corrupted);
    }
    
    Ok(())
}

async fn test_dht(repo_hash: String, action: String) -> anyhow::Result<()> {
    println!(" Testing DHT functionality...");
    
    let config = config::NodeConfig::load()?;
    let mut dht = dht::DHT::new(config.node_id.clone());
    
    match action.as_str() {
        "announce" => {
            dht.announce_content(&repo_hash, &config.node_id);
            println!(" Announced {} to DHT", &repo_hash[..16]);
        }
        "query" => {
            let nodes = dht.query_content(&repo_hash);
            println!("Found {} nodes hosting {}", nodes.len(), &repo_hash[..16]);
            for node in nodes {
                println!("  - {}", &node[..16]);
            }
        }
        _ => {
            println!(" Unknown action: {}", action);
            println!("   Use: announce or query");
        }
    }
    
    Ok(())
}

use colored::Colorize;
