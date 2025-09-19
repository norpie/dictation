use anyhow::Result;
use shared::ipc::{ClientMessage, DaemonMessage, protocol};
use std::sync::mpsc;
use tokio::net::UnixStream;
use uuid::Uuid;

use crate::app::UiMessage;

pub fn start_daemon_communication_thread(tx: mpsc::Sender<UiMessage>) {
    std::thread::spawn(move || {
        daemon_communication_thread(tx);
    });
}

fn daemon_communication_thread(tx: mpsc::Sender<UiMessage>) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // Check daemon status
        match check_daemon_status().await {
            Ok(true) => {
                let _ = tx.send(UiMessage::DaemonConnected(true));

                // Start recording
                match send_start_recording(tx.clone()).await {
                    Ok((session_id, stream)) => {
                        let _ = tx.send(UiMessage::RecordingStarted(session_id));

                        // Listen for daemon messages on the same connection
                        listen_for_daemon_messages(tx, session_id, stream).await;
                    }
                    Err(e) => {
                        let _ = tx.send(UiMessage::Error(format!("Failed to start recording: {}", e)));
                    }
                }
            }
            _ => {
                // Daemon not available - show notification and exit
                let _ = std::process::Command::new("notify-send")
                    .arg("Voice Dictation")
                    .arg("Daemon not available. Please start the daemon first.")
                    .arg("--urgency=critical")
                    .spawn();

                std::process::exit(1);
            }
        }
    });
}

async fn check_daemon_status() -> Result<bool> {
    let socket_path = "/tmp/dictation.sock";

    match UnixStream::connect(socket_path).await {
        Ok(mut stream) => {
            if protocol::send_message(&mut stream, &ClientMessage::GetStatus).await.is_ok() {
                match tokio::time::timeout(
                    tokio::time::Duration::from_secs(2),
                    protocol::receive_message::<DaemonMessage>(&mut stream)
                ).await {
                    Ok(Ok(DaemonMessage::Status(_))) => Ok(true),
                    _ => Ok(false),
                }
            } else {
                Ok(false)
            }
        }
        Err(_) => Ok(false),
    }
}

async fn send_start_recording(tx: mpsc::Sender<UiMessage>) -> Result<(Uuid, UnixStream)> {
    let socket_path = "/tmp/dictation.sock";
    let mut stream = UnixStream::connect(socket_path).await?;

    protocol::send_message(&mut stream, &ClientMessage::StartRecording).await?;

    // The daemon might send ModelLoading -> ModelLoaded -> RecordingStarted
    // We need to wait for RecordingStarted specifically
    loop {
        match protocol::receive_message::<DaemonMessage>(&mut stream).await? {
            DaemonMessage::RecordingStarted(session_id) => {
                return Ok((session_id, stream));
            }
            DaemonMessage::ModelLoading => {
                // Forward model loading message to UI
                let _ = tx.send(UiMessage::ModelLoading);
                continue;
            }
            DaemonMessage::ModelLoaded => {
                // Forward model loaded message to UI
                let _ = tx.send(UiMessage::ModelLoaded);
                continue;
            }
            DaemonMessage::Error(error) => {
                anyhow::bail!("Daemon error: {}", error);
            }
            other => {
                anyhow::bail!("Unexpected response from daemon: {:?}", other);
            }
        }
    }
}

pub async fn send_stop_recording() -> Result<()> {
    let socket_path = "/tmp/dictation.sock";
    let mut stream = UnixStream::connect(socket_path).await?;

    protocol::send_message(&mut stream, &ClientMessage::StopRecording).await?;
    Ok(())
}

async fn listen_for_daemon_messages(tx: mpsc::Sender<UiMessage>, session_id: Uuid, mut stream: UnixStream) {
    loop {
        match protocol::receive_message::<DaemonMessage>(&mut stream).await {
            Ok(message) => {
                match message {
                    DaemonMessage::TranscriptionUpdate { session_id: msg_session_id, partial_text, is_final } => {
                        if msg_session_id == session_id {
                            let _ = tx.send(UiMessage::TranscriptionUpdate(partial_text, is_final));
                        }
                    }
                    DaemonMessage::TranscriptionComplete(session) => {
                        if session.id == session_id {
                            let _ = tx.send(UiMessage::TranscriptionComplete(session.text));
                        }
                    }
                    DaemonMessage::RecordingStopped => {
                        let _ = tx.send(UiMessage::RecordingStopped);
                        return; // Exit listen loop
                    }
                    // Real-time feedback messages (audio level removed)
                    // Model management messages
                    DaemonMessage::ModelLoading => {
                        let _ = tx.send(UiMessage::ModelLoading);
                    }
                    DaemonMessage::ModelLoaded => {
                        let _ = tx.send(UiMessage::ModelLoaded);
                    }
                    DaemonMessage::ModelUnloaded => {
                        let _ = tx.send(UiMessage::ModelUnloaded);
                    }
                    DaemonMessage::SessionCleared => {
                        let _ = tx.send(UiMessage::SessionCleared);
                    }
                    DaemonMessage::Error(error) => {
                        let _ = tx.send(UiMessage::Error(error));
                    }
                    _ => {}
                }
            }
            Err(e) => {
                let _ = tx.send(UiMessage::Error(format!("Connection lost: {}", e)));
                return;
            }
        }
    }
}