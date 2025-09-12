use anyhow::Result;
use log::{info, error, warn};
use shared::{Config, ClientMessage, DaemonMessage, protocol};
use tokio::net::{UnixListener, UnixStream};
use tokio::fs;
use std::sync::Arc;
use std::collections::HashMap;
use uuid::Uuid;

mod daemon;
mod whisper;

use daemon::Daemon;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    info!("Starting dictation daemon");
    
    // Load configuration
    let config = Config::load()?;
    info!("Loaded configuration from {:?}", Config::config_file()?);
    
    // Remove existing socket if it exists
    if config.ipc.socket_path.exists() {
        fs::remove_file(&config.ipc.socket_path).await?;
        warn!("Removed existing socket at {:?}", config.ipc.socket_path);
    }
    
    // Create Unix domain socket listener
    let listener = UnixListener::bind(&config.ipc.socket_path)?;
    info!("IPC server listening on {:?}", config.ipc.socket_path);
    
    // Initialize daemon state
    let daemon = Arc::new(Daemon::new(config)?);
    
    // Accept client connections
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let daemon = daemon.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_client(stream, daemon).await {
                        error!("Client handler error: {}", e);
                    }
                });
            }
            Err(e) => {
                error!("Failed to accept client connection: {}", e);
            }
        }
    }
}

async fn handle_client(mut stream: UnixStream, daemon: Arc<Daemon>) -> Result<()> {
    info!("New client connected");
    
    loop {
        match protocol::receive_message::<ClientMessage>(&mut stream).await {
            Ok(message) => {
                let response = daemon.handle_message(message).await;
                if let Err(e) = protocol::send_message(&mut stream, &response).await {
                    error!("Failed to send response to client: {}", e);
                    break;
                }
            }
            Err(e) => {
                info!("Client disconnected: {}", e);
                break;
            }
        }
    }
    
    Ok(())
}