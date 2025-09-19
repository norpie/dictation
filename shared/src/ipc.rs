use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::types::{TranscriptionSession, AudioChunk};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    StartRecording,
    StopRecording,
    StreamAudio(AudioChunk),
    GetStatus,
    ClearSession,        // Clear any buffered/old transcriptions
    SetSensitivity(f32), // Adjust voice detection sensitivity (0.0-1.0)
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonMessage {
    RecordingStarted(Uuid),
    RecordingStopped,

    // Enhanced transcription messages
    TranscriptionUpdate {
        session_id: Uuid,
        partial_text: String,
        is_final: bool,  // Is this segment final?
    },
    TranscriptionComplete(TranscriptionSession),

    // Real-time feedback
    AudioLevel(f32),           // Current audio level (0.0-1.0)
    VoiceActivityDetected,     // Voice detected, processing will start
    VoiceActivityEnded,        // Voice stopped, finishing segment
    ProcessingStarted,         // Started transcribing audio chunk
    ProcessingComplete,        // Finished transcribing chunk

    // Status and session management
    Error(String),
    Status(DaemonStatus),
    SessionCleared,            // Confirm session was cleared
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub model_loaded: bool,
    pub active_sessions: Vec<Uuid>,
    pub uptime: std::time::Duration,
    pub audio_device: String,      // Current audio device name
    pub buffer_size: usize,        // Current audio buffer size
    pub vad_sensitivity: f32,      // Voice detection sensitivity (0.0-1.0)
}

pub mod protocol {
    use super::*;
    use anyhow::Result;
    use tokio::net::UnixStream;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    
    pub async fn send_message<T: Serialize>(
        stream: &mut UnixStream,
        message: &T
    ) -> Result<()> {
        let serialized = rmp_serde::to_vec(message)?;
        let len = serialized.len() as u32;

        stream.write_all(&len.to_le_bytes()).await?;
        stream.write_all(&serialized).await?;
        stream.flush().await?;

        Ok(())
    }

    pub async fn receive_message<T: for<'de> Deserialize<'de>>(
        stream: &mut UnixStream
    ) -> Result<T> {
        let mut len_bytes = [0u8; 4];
        stream.read_exact(&mut len_bytes).await?;
        let len = u32::from_le_bytes(len_bytes) as usize;

        let mut buffer = vec![0u8; len];
        stream.read_exact(&mut buffer).await?;

        let message = rmp_serde::from_slice(&buffer)?;
        Ok(message)
    }
}