"""
Configuration loading for the dictation daemon.
"""

import os
import tomllib
from pathlib import Path
from dataclasses import dataclass
import logging

logger = logging.getLogger(__name__)

@dataclass
class WhisperConfig:
    model: str = "distil-large-v3"
    model_timeout_seconds: int = 300
    language: str = "en"
    fuzzy_match_threshold: float = 0.8

@dataclass
class UIConfig:
    auto_copy: bool = True

@dataclass
class Config:
    whisper: WhisperConfig
    ui: UIConfig

def load_config() -> Config:
    """Load configuration from ~/.config/dictation/config.toml"""
    config_path = Path.home() / ".config" / "dictation" / "config.toml"

    # Default config
    config = Config(
        whisper=WhisperConfig(),
        ui=UIConfig()
    )

    if not config_path.exists():
        logger.info(f"No config file found at {config_path}, using defaults")
        return config

    try:
        with open(config_path, "rb") as f:
            toml_data = tomllib.load(f)

        # Load whisper config
        if "whisper" in toml_data:
            whisper_data = toml_data["whisper"]
            config.whisper.model = whisper_data.get("model", config.whisper.model)
            config.whisper.model_timeout_seconds = whisper_data.get("model_timeout_seconds", config.whisper.model_timeout_seconds)
            config.whisper.language = whisper_data.get("language", config.whisper.language)
            config.whisper.fuzzy_match_threshold = whisper_data.get("fuzzy_match_threshold", config.whisper.fuzzy_match_threshold)

        # Load UI config
        if "ui" in toml_data:
            ui_data = toml_data["ui"]
            config.ui.auto_copy = ui_data.get("auto_copy", config.ui.auto_copy)

        logger.info(f"Loaded config from {config_path}")
        logger.info(f"Model: {config.whisper.model}, Timeout: {config.whisper.model_timeout_seconds}s, Auto-copy: {config.ui.auto_copy}")

    except Exception as e:
        logger.error(f"Failed to load config from {config_path}: {e}")
        logger.info("Using default configuration")

    return config