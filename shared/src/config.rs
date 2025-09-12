use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub whisper: WhisperConfig,
    pub audio: AudioConfig,
    pub ui: UiConfig,
    pub ipc: IpcConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhisperConfig {
    pub model_path: PathBuf,
    pub model_timeout_seconds: u64,
    pub vad_threshold: f32,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub device: Option<String>,
    pub sample_rate: u32,
    pub channels: u16,
    pub buffer_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub popup_width: i32,
    pub popup_height: i32,
    pub auto_copy: bool,
    pub show_notifications: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcConfig {
    pub socket_path: PathBuf,
    pub timeout_seconds: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            whisper: WhisperConfig {
                model_path: PathBuf::from("models/ggml-base.en.bin"),
                model_timeout_seconds: 300, // 5 minutes
                vad_threshold: 0.1,
                language: Some("en".to_string()),
            },
            audio: AudioConfig {
                device: None, // Use default device
                sample_rate: 16000,
                channels: 1,
                buffer_size: 1024,
            },
            ui: UiConfig {
                popup_width: 400,
                popup_height: 200,
                auto_copy: false,
                show_notifications: true,
            },
            ipc: IpcConfig {
                socket_path: PathBuf::from("/tmp/dictation.sock"),
                timeout_seconds: 30,
            },
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        // TODO: Load from XDG config directory
        // For now, return default config
        Ok(Self::default())
    }
    
    pub fn save(&self) -> Result<()> {
        // TODO: Save to XDG config directory
        Ok(())
    }
}