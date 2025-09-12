use anyhow::Result;
use log::{info, error, debug};
use shared::{Config, ClientMessage, DaemonMessage, DaemonStatus, TranscriptionSession, AudioChunk};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, Instant};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::whisper::WhisperManager;

pub struct AudioBuffer {
    data: Vec<f32>,
    sample_rate: u32,
    channels: u16,
    last_chunk_time: SystemTime,
}

impl AudioBuffer {
    fn new() -> Self {
        Self {
            data: Vec::new(),
            sample_rate: 16000,
            channels: 1,
            last_chunk_time: SystemTime::now(),
        }
    }
    
    fn append_chunk(&mut self, chunk: &AudioChunk) {
        self.data.extend_from_slice(&chunk.data);
        self.sample_rate = chunk.sample_rate;
        self.channels = chunk.channels;
        self.last_chunk_time = chunk.timestamp;
    }
    
    fn duration_seconds(&self) -> f32 {
        if self.sample_rate == 0 || self.channels == 0 {
            return 0.0;
        }
        self.data.len() as f32 / (self.sample_rate * self.channels as u32) as f32
    }
    
    fn is_silent_timeout(&self, timeout_seconds: f32) -> bool {
        self.last_chunk_time.elapsed().unwrap_or_default().as_secs_f32() > timeout_seconds
    }
    
    fn get_all_audio(&self) -> Vec<f32> {
        // Return all audio data for final transcription
        self.data.clone()
    }
}

pub struct Daemon {
    config: Config,
    whisper_manager: Arc<RwLock<WhisperManager>>,
    active_sessions: Arc<RwLock<HashMap<Uuid, TranscriptionSession>>>,
    audio_buffers: Arc<RwLock<HashMap<Uuid, AudioBuffer>>>,
    start_time: Instant,
}

impl Daemon {
    pub fn new(config: Config) -> Result<Self> {
        let whisper_manager = Arc::new(RwLock::new(WhisperManager::new(&config.whisper)?));
        
        Ok(Self {
            config,
            whisper_manager,
            active_sessions: Arc::new(RwLock::new(HashMap::new())),
            audio_buffers: Arc::new(RwLock::new(HashMap::new())),
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
        
        // Store the session and create audio buffer
        {
            let mut sessions = self.active_sessions.write().await;
            sessions.insert(session_id, session);
        }
        {
            let mut buffers = self.audio_buffers.write().await;
            buffers.insert(session_id, AudioBuffer::new());
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
        
        // Process any remaining audio in buffers before stopping
        let final_transcriptions = {
            let mut buffers = self.audio_buffers.write().await;
            let mut sessions = self.active_sessions.write().await;
            let mut results = Vec::new();
            
            for (session_id, buffer) in buffers.drain() {
                if !buffer.data.is_empty() {
                    info!("Processing final audio for session {}: {:.1}s of audio", 
                        session_id, buffer.duration_seconds());
                    
                    // Transcribe the final audio
                    let audio_data = buffer.get_all_audio();
                    let mut whisper = self.whisper_manager.write().await;
                    match whisper.transcribe_audio(&audio_data).await {
                        Ok(transcription) => {
                            if let Some(session) = sessions.get_mut(&session_id) {
                                session.text = transcription.clone();
                                session.status = shared::SessionStatus::Completed;
                                results.push(session.clone());
                            }
                        }
                        Err(e) => {
                            error!("Failed to transcribe final audio for session {}: {}", session_id, e);
                            if let Some(session) = sessions.get_mut(&session_id) {
                                session.status = shared::SessionStatus::Failed(e.to_string());
                                results.push(session.clone());
                            }
                        }
                    }
                }
            }
            
            sessions.clear();
            results
        };
        
        // Return the final transcription if there's one session, otherwise just stopped
        if let Some(session) = final_transcriptions.into_iter().next() {
            DaemonMessage::TranscriptionComplete(session)
        } else {
            DaemonMessage::RecordingStopped
        }
    }
    
    async fn handle_audio_chunk(&self, audio_chunk: AudioChunk) -> DaemonMessage {
        debug!("Received audio chunk for session {} with {} samples", 
            audio_chunk.session_id, audio_chunk.data.len());
        
        // Check if session exists
        let session_exists = {
            let sessions = self.active_sessions.read().await;
            sessions.contains_key(&audio_chunk.session_id)
        };
        
        if !session_exists {
            return DaemonMessage::Error("Session not found".to_string());
        }
        
        // Just add audio to buffer - no transcription during streaming
        {
            let mut buffers = self.audio_buffers.write().await;
            if let Some(buffer) = buffers.get_mut(&audio_chunk.session_id) {
                buffer.append_chunk(&audio_chunk);
                debug!("Buffer now has {:.1}s of audio", buffer.duration_seconds());
            } else {
                error!("Audio buffer not found for session {}", audio_chunk.session_id);
                return DaemonMessage::Error("Audio buffer not found".to_string());
            }
        }
        
        // Just acknowledge receipt
        DaemonMessage::TranscriptionUpdate {
            session_id: audio_chunk.session_id,
            partial_text: "".to_string(),
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