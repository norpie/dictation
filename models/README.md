# Whisper Models

This directory contains Whisper model files for speech recognition.

## Downloading Models

You can download Whisper models from the official repository. Common models include:

### Recommended Models
- `ggml-base.en.bin` - Good balance of speed and accuracy for English
- `ggml-small.en.bin` - Smaller, faster model for English
- `ggml-base.bin` - Multilingual base model

### Download Instructions

```bash
# Download base English model (recommended for testing)
curl -L -o models/ggml-base.en.bin https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin

# Download small English model (faster)
curl -L -o models/ggml-small.en.bin https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin

# Download multilingual base model
curl -L -o models/ggml-base.bin https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin
```

### Configuration

Update your configuration file (`~/.config/dictation/config.yaml`) to point to the downloaded model:

```yaml
whisper:
  model_path: "models/ggml-base.en.bin"
  model_timeout_seconds: 300
  vad_threshold: 0.1
  language: "en"
```

## Model Sizes

| Model | Size | Speed | Accuracy |
|-------|------|-------|----------|
| tiny  | 39 MB | Fastest | Lower |
| base  | 142 MB | Fast | Good |
| small | 244 MB | Medium | Better |
| medium| 769 MB | Slower | Great |
| large | 1550 MB | Slowest | Best |

Choose based on your performance requirements and available system resources.