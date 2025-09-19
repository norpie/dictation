use anyhow::Result;
use clap::Parser;
use log::{info, error, debug, warn};
use shared::{Config, ClientMessage, DaemonMessage, protocol};
use tokio::net::UnixStream;
use tokio::time::{timeout, Duration};

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

            // If this is a start recording command, listen for transcription updates
            if args.start {
                if let Err(e) = listen_for_transcription(&mut stream).await {
                    error!("Failed to listen for transcription: {}", e);
                    return Err(e);
                }
            }
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
        DaemonMessage::TranscriptionUpdate { session_id: _, partial_text, is_final: _ } => {
            println!("Partial transcription: {}", partial_text);
        }
        DaemonMessage::TranscriptionComplete(session) => {
            println!("✓ Transcription complete: {}", session.text);
        }
        _ => {
            // Ignore other message types for this simple client
        }
    }
    
    Ok(())
}

async fn listen_for_transcription(stream: &mut UnixStream) -> Result<()> {
    println!("🎤 Recording... Speak now! (RealtimeSTT will handle audio capture)");

    // Listen for transcription updates from daemon
    loop {
        let timeout_duration = Duration::from_secs(60); // 60 second timeout

        match timeout(timeout_duration, protocol::receive_message::<DaemonMessage>(stream)).await {
            Ok(Ok(response)) => {
                match response {
                    DaemonMessage::TranscriptionUpdate { session_id: _, partial_text, is_final: _ } => {
                        if !partial_text.is_empty() {
                            print!("\r🔄 Partial: {}", partial_text);
                            use std::io::{self, Write};
                            io::stdout().flush().unwrap();
                        }
                    }
                    DaemonMessage::TranscriptionComplete(session) => {
                        println!("\n✅ Final: {}", session.text);
                        break;
                    }
                    DaemonMessage::RecordingStopped => {
                        println!("\n🔚 Recording stopped");
                        break;
                    }
                    DaemonMessage::Error(error) => {
                        error!("Daemon error during recording: {}", error);
                        break;
                    }
                    other => {
                        debug!("Received unexpected message: {:?}", other);
                    }
                }
            }
            Ok(Err(e)) => {
                error!("Error receiving message: {}", e);
                break;
            }
            Err(_) => {
                warn!("Timeout waiting for transcription");
                break;
            }
        }
    }

    Ok(())
}