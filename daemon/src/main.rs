use anyhow::Result;
use log::info;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    info!("Starting dictation daemon");
    
    // TODO: Initialize Whisper model
    // TODO: Set up IPC server
    // TODO: Handle audio capture
    
    Ok(())
}