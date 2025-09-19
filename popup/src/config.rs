use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Deserialize, Serialize, Default, Clone)]
pub struct Config {
    pub whisper: Option<WhisperConfig>,
    pub ui: Option<UIConfig>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct WhisperConfig {
    pub model: Option<String>,
    pub model_timeout_seconds: Option<u32>,
    pub language: Option<String>,
    pub fuzzy_match_threshold: Option<f32>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct UIConfig {
    pub auto_copy: Option<bool>,
    pub auto_close_after_copy: Option<bool>,
}

impl Config {
    pub fn auto_copy(&self) -> bool {
        self.ui.as_ref()
            .and_then(|ui| ui.auto_copy)
            .unwrap_or(true)
    }

    pub fn auto_close_after_copy(&self) -> bool {
        self.ui.as_ref()
            .and_then(|ui| ui.auto_close_after_copy)
            .unwrap_or(true)
    }

    pub fn model(&self) -> String {
        self.whisper.as_ref()
            .and_then(|w| w.model.clone())
            .unwrap_or_else(|| "distil-large-v3".to_string())
    }

    pub fn model_timeout_seconds(&self) -> u32 {
        self.whisper.as_ref()
            .and_then(|w| w.model_timeout_seconds)
            .unwrap_or(300)
    }

    pub fn language(&self) -> String {
        self.whisper.as_ref()
            .and_then(|w| w.language.clone())
            .unwrap_or_else(|| "en".to_string())
    }

    pub fn fuzzy_match_threshold(&self) -> f32 {
        self.whisper.as_ref()
            .and_then(|w| w.fuzzy_match_threshold)
            .unwrap_or(0.8)
    }
}

pub fn load_config() -> Config {
    let config_path = dirs::home_dir()
        .map(|home| home.join(".config").join("dictation").join("config.toml"))
        .unwrap_or_default();

    if let Ok(content) = fs::read_to_string(&config_path) {
        toml::from_str(&content).unwrap_or_default()
    } else {
        Config::default()
    }
}

pub fn save_config(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let config_dir = dirs::home_dir()
        .map(|home| home.join(".config").join("dictation"))
        .ok_or("Could not find home directory")?;

    // Create config directory if it doesn't exist
    fs::create_dir_all(&config_dir)?;

    let config_path = config_dir.join("config.toml");
    let toml_string = toml::to_string_pretty(config)?;
    fs::write(&config_path, toml_string)?;

    Ok(())
}