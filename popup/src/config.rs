use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Deserialize, Serialize, Default, Clone)]
pub struct Config {
    pub ui: Option<UIConfig>,
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