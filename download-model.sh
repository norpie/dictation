#!/bin/bash
set -e

# Download script for Whisper models

MODEL_DIR="models"
BASE_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main"

# Create models directory if it doesn't exist
mkdir -p "$MODEL_DIR"

# Default model to download
MODEL_NAME="${1:-ggml-base.en.bin}"
MODEL_PATH="$MODEL_DIR/$MODEL_NAME"

echo "Downloading Whisper model: $MODEL_NAME"
echo "Destination: $MODEL_PATH"

# Check if model already exists
if [ -f "$MODEL_PATH" ]; then
    echo "Model already exists at $MODEL_PATH"
    echo "Delete the file if you want to re-download it."
    exit 0
fi

# Download the model
echo "Downloading from $BASE_URL/$MODEL_NAME..."
curl -L --progress-bar -o "$MODEL_PATH" "$BASE_URL/$MODEL_NAME"

echo "✓ Model downloaded successfully: $MODEL_PATH"
echo ""
echo "To use this model, make sure your config file (~/.config/dictation/config.yaml) has:"
echo "whisper:"
echo "  model_path: \"$MODEL_PATH\""
echo ""
echo "Available models:"
echo "  ggml-tiny.en.bin    (39 MB)   - Fastest, lowest accuracy"
echo "  ggml-base.en.bin    (142 MB)  - Good balance (default)"
echo "  ggml-small.en.bin   (244 MB)  - Better accuracy"
echo "  ggml-medium.en.bin  (769 MB)  - High accuracy"
echo "  ggml-large-v1.bin   (1550 MB) - Best accuracy"
echo ""
echo "Usage: $0 [model-name]"
echo "Example: $0 ggml-small.en.bin"