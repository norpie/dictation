use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;

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
    pub fn config_dir() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .context("Failed to get config directory")?
            .join("dictation");
        
        if !config_dir.exists() {
            fs::create_dir_all(&config_dir)
                .context("Failed to create config directory")?;
        }
        
        Ok(config_dir)
    }
    
    pub fn config_file() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.yaml"))
    }
    
    pub fn load() -> Result<Self> {
        let config_file = Self::config_file()?;
        
        if config_file.exists() {
            let content = fs::read_to_string(&config_file)
                .with_context(|| format!("Failed to read config file: {}", config_file.display()))?;
            
            let config: Config = serde_yaml::from_str(&content)
                .with_context(|| format!("Failed to parse config file: {}", config_file.display()))?;
                
            Ok(config)
        } else {
            // Create default config file
            let default_config = Self::default();
            default_config.save()?;
            Ok(default_config)
        }
    }
    
    pub fn save(&self) -> Result<()> {
        let config_file = Self::config_file()?;
        let content = serde_yaml::to_string(self)
            .context("Failed to serialize config")?;
            
        fs::write(&config_file, content)
            .with_context(|| format!("Failed to write config file: {}", config_file.display()))?;
            
        Ok(())
    }
}