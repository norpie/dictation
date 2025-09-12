use anyhow::Result;
use log::{info, error, debug};
use shared::{Config, ClientMessage, DaemonMessage, DaemonStatus, TranscriptionSession};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, Instant};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::whisper::WhisperManager;

pub struct Daemon {
    config: Config,
    whisper_manager: Arc<RwLock<WhisperManager>>,
    active_sessions: Arc<RwLock<HashMap<Uuid, TranscriptionSession>>>,
    start_time: Instant,
}

impl Daemon {
    pub fn new(config: Config) -> Result<Self> {
        let whisper_manager = Arc::new(RwLock::new(WhisperManager::new(&config.whisper)?));
        
        Ok(Self {
            config,
            whisper_manager,
            active_sessions: Arc::new(RwLock::new(HashMap::new())),
            start_time: Instant::now(),
        })
    }
    
    pub async fn handle_message(&self, message: ClientMessage) -> DaemonMessage {
        debug!("Handling client message: {:?}", message);
        
        match message {
            ClientMessage::StartRecording => {
                self.start_recording().await
            }
            ClientMessage::StopRecording => {
                self.stop_recording().await
            }
            ClientMessage::StreamAudio(audio_chunk) => {
                self.handle_audio_chunk(audio_chunk).await
            }
            ClientMessage::GetStatus => {
                self.get_status().await
            }
            ClientMessage::Shutdown => {
                info!("Received shutdown command");
                std::process::exit(0);
            }
        }
    }
    
    async fn start_recording(&self) -> DaemonMessage {
        info!("Starting new recording session");
        
        let session = TranscriptionSession::new();
        let session_id = session.id;
        
        // Store the session
        {
            let mut sessions = self.active_sessions.write().await;
            sessions.insert(session_id, session);
        }
        
        // Ensure whisper model is loaded
        {
            let mut whisper = self.whisper_manager.write().await;
            if let Err(e) = whisper.ensure_loaded().await {
                error!("Failed to load Whisper model: {}", e);
                return DaemonMessage::Error(format!("Failed to load Whisper model: {}", e));
            }
        }
        
        DaemonMessage::RecordingStarted(session_id)
    }
    
    async fn stop_recording(&self) -> DaemonMessage {
        info!("Stopping recording sessions");
        
        // For now, just clear all active sessions
        // In the future, we'll want to finalize transcription
        {
            let mut sessions = self.active_sessions.write().await;
            sessions.clear();
        }
        
        DaemonMessage::RecordingStopped
    }
    
    async fn handle_audio_chunk(&self, audio_chunk: shared::AudioChunk) -> DaemonMessage {
        debug!("Received audio chunk for session {}", audio_chunk.session_id);
        
        // Check if session exists
        let session_exists = {
            let sessions = self.active_sessions.read().await;
            sessions.contains_key(&audio_chunk.session_id)
        };
        
        if !session_exists {
            return DaemonMessage::Error("Session not found".to_string());
        }
        
        // TODO: Process audio chunk with Whisper
        // For now, just acknowledge receipt
        DaemonMessage::TranscriptionUpdate {
            session_id: audio_chunk.session_id,
            partial_text: "[Audio received - transcription not yet implemented]".to_string(),
        }
    }
    
    async fn get_status(&self) -> DaemonMessage {
        let whisper_loaded = {
            let whisper = self.whisper_manager.read().await;
            whisper.is_loaded()
        };
        
        let active_session_ids = {
            let sessions = self.active_sessions.read().await;
            sessions.keys().copied().collect()
        };
        
        let status = DaemonStatus {
            model_loaded: whisper_loaded,
            active_sessions: active_session_ids,
            uptime: self.start_time.elapsed(),
        };
        
        DaemonMessage::Status(status)
    }
}