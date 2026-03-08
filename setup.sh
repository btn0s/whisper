#!/bin/bash
set -e

WHISPR_DIR="$HOME/.whispr"
MODEL_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin"
MODEL_PATH="$WHISPR_DIR/ggml-base.en.bin"

echo "=== Whispr Setup ==="

# Download whisper model
if [ ! -f "$MODEL_PATH" ]; then
    echo "Downloading Whisper base.en model..."
    mkdir -p "$WHISPR_DIR"
    curl -L "$MODEL_URL" -o "$MODEL_PATH"
    echo "Model saved to $MODEL_PATH"
else
    echo "Whisper model already exists at $MODEL_PATH"
fi

# Check Ollama
if ! command -v ollama &> /dev/null; then
    echo ""
    echo "WARNING: Ollama is not installed."
    echo "Install it from https://ollama.com and then run:"
    echo "  ollama pull qwen3.5:8b"
else
    echo ""
    echo "Pulling Qwen model via Ollama..."
    ollama pull qwen3.5:8b
fi

echo ""
echo "Setup complete! Run 'cargo tauri dev' from the project root."
