use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionSession {
    pub id: Uuid,
    pub status: SessionStatus,
    pub text: String,
    pub confidence: Option<f32>,
    pub created_at: std::time::SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionStatus {
    Recording,
    Processing,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioChunk {
    pub session_id: Uuid,
    pub data: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    pub timestamp: std::time::SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub path: std::path::PathBuf,
    pub loaded: bool,
    pub last_used: Option<std::time::SystemTime>,
}

impl TranscriptionSession {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            status: SessionStatus::Recording,
            text: String::new(),
            confidence: None,
            created_at: std::time::SystemTime::now(),
        }
    }
}