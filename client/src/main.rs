use anyhow::Result;
use clap::Parser;
use log::{info, error, debug, warn};
use shared::{Config, ClientMessage, DaemonMessage, protocol};
use tokio::net::UnixStream;
use tokio::time::{timeout, Duration};
use uuid::Uuid;

mod audio;

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
            
            // If this is a start recording command, begin audio capture
            if args.start {
                if let Err(e) = start_audio_capture(&config, session_id, &mut stream).await {
                    error!("Failed to start audio capture: {}", e);
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
        DaemonMessage::TranscriptionUpdate { session_id: _, partial_text } => {
            println!("Partial transcription: {}", partial_text);
        }
        DaemonMessage::TranscriptionComplete(session) => {
            println!("✓ Transcription complete: {}", session.text);
        }
    }
    
    Ok(())
}

async fn start_audio_capture(
    config: &Config,
    session_id: Uuid,
    stream: &mut UnixStream,
) -> Result<()> {
    info!("Starting audio capture for session {}", session_id);
    
    let mut audio_capture = audio::AudioCapture::new(&config.audio)?;
    let mut audio_rx = audio_capture.start_recording(session_id)?;
    
    println!("🎤 Recording audio... Press Ctrl+C to stop");
    
    // Stream audio chunks to daemon
    let mut chunk_count = 0;
    let recording_timeout = Duration::from_secs(30); // 30 second max recording
    
    let audio_streaming = async {
        let mut exit_reason = "Audio stream ended";
        
        while let Some(audio_chunk) = audio_rx.recv().await {
            chunk_count += 1;
            debug!("Sending audio chunk {} with {} samples", chunk_count, audio_chunk.data.len());
            
            // Send audio chunk to daemon
            let message = ClientMessage::StreamAudio(audio_chunk);
            if let Err(e) = protocol::send_message(stream, &message).await {
                error!("Failed to send audio chunk: {}", e);
                exit_reason = "Failed to send audio to daemon";
                break;
            }
            
            // Check for daemon response (transcription updates)
            if let Ok(response_result) = timeout(Duration::from_millis(10), protocol::receive_message::<DaemonMessage>(stream)).await {
                match response_result {
                    Ok(DaemonMessage::TranscriptionUpdate { session_id: _, partial_text }) => {
                        if !partial_text.is_empty() {
                            println!("\r🔄 Partial: {}", partial_text);
                        }
                    }
                    Ok(DaemonMessage::TranscriptionComplete(session)) => {
                        println!("\n✅ Final: {}", session.text);
                        exit_reason = "Transcription completed by daemon";
                        break;
                    }
                    Ok(DaemonMessage::Error(error)) => {
                        error!("Daemon error during recording: {}", error);
                        exit_reason = "Daemon reported an error";
                        break;
                    }
                    Ok(other) => {
                        debug!("Received other message during recording: {:?}", other);
                    }
                    Err(e) => {
                        debug!("No response from daemon: {}", e);
                        // Continue recording
                    }
                }
            }
        }
        
        info!("Audio streaming completed after {} chunks. Reason: {}", chunk_count, exit_reason);
        exit_reason
    };
    
    // Run audio streaming with timeout
    let final_reason = match timeout(recording_timeout, audio_streaming).await {
        Ok(reason) => {
            info!("Recording completed successfully");
            reason
        }
        Err(_) => {
            warn!("Recording timed out after {} seconds", recording_timeout.as_secs());
            "30-second timeout reached"
        }
    };
    
    println!("🔚 Recording ended: {}", final_reason);
    
    // Stop recording
    audio_capture.stop_recording();
    
    // Send stop recording message to daemon
    let stop_message = ClientMessage::StopRecording;
    protocol::send_message(stream, &stop_message).await?;
    
    // Wait for confirmation
    if let Ok(response) = timeout(Duration::from_secs(5), protocol::receive_message::<DaemonMessage>(stream)).await {
        match response? {
            DaemonMessage::RecordingStopped => {
                println!("✓ Recording stopped");
            }
            DaemonMessage::TranscriptionComplete(session) => {
                println!("✅ Final transcription: {}", session.text);
            }
            other => {
                debug!("Received unexpected stop response: {:?}", other);
            }
        }
    }
    
    Ok(())
}