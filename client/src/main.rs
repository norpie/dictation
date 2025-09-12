use anyhow::Result;
use clap::Parser;
use log::{info, error};
use shared::{Config, ClientMessage, DaemonMessage, protocol};
use tokio::net::UnixStream;

#[derive(Parser)]
#[command(name = "dictation-client")]
#[command(about = "Trigger dictation recording")]
struct Args {
    #[arg(short, long)]
    start: bool,
    
    #[arg(long)]
    stop: bool,
    
    #[arg(long)]
    status: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();
    
    info!("Dictation client started");
    
    // Load configuration to get socket path
    let config = Config::load()?;
    
    // Connect to daemon
    let mut stream = match UnixStream::connect(&config.ipc.socket_path).await {
        Ok(stream) => {
            info!("Connected to daemon at {:?}", config.ipc.socket_path);
            stream
        }
        Err(e) => {
            error!("Failed to connect to daemon: {}. Is the daemon running?", e);
            return Err(e.into());
        }
    };
    
    // Send appropriate command
    let message = if args.start {
        ClientMessage::StartRecording
    } else if args.stop {
        ClientMessage::StopRecording
    } else if args.status {
        ClientMessage::GetStatus
    } else {
        error!("Please specify --start, --stop, or --status");
        return Ok(());
    };
    
    // Send message to daemon
    protocol::send_message(&mut stream, &message).await?;
    info!("Sent message to daemon: {:?}", message);
    
    // Receive response
    let response: DaemonMessage = protocol::receive_message(&mut stream).await?;
    info!("Received response: {:?}", response);
    
    // Handle response
    match response {
        DaemonMessage::RecordingStarted(session_id) => {
            println!("✓ Recording started with session ID: {}", session_id);
        }
        DaemonMessage::RecordingStopped => {
            println!("✓ Recording stopped");
        }
        DaemonMessage::Status(status) => {
            println!("Daemon Status:");
            println!("  Model loaded: {}", status.model_loaded);
            println!("  Active sessions: {:?}", status.active_sessions);
            println!("  Uptime: {:?}", status.uptime);
        }
        DaemonMessage::Error(error) => {
            error!("Daemon error: {}", error);
        }
        _ => {
            info!("Unexpected response: {:?}", response);
        }
    }
    
    Ok(())
}