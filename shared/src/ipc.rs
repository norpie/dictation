use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::types::{TranscriptionSession, AudioChunk};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    StartRecording,
    StopRecording,
    StreamAudio(AudioChunk),
    GetStatus,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonMessage {
    RecordingStarted(Uuid),
    RecordingStopped,
    TranscriptionUpdate { 
        session_id: Uuid, 
        partial_text: String 
    },
    TranscriptionComplete(TranscriptionSession),
    Error(String),
    Status(DaemonStatus),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub model_loaded: bool,
    pub active_sessions: Vec<Uuid>,
    pub uptime: std::time::Duration,
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