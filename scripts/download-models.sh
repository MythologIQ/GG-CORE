#!/bin/bash
# Veritas SDR Model Download Script
# Downloads recommended GGUF models for testing and production

set -e

TIER="${1:-default}"
MODELS_DIR="${2:-models}"

echo "Veritas SDR Model Downloader"
echo "============================="
echo ""

# Ensure models directory exists
mkdir -p "$MODELS_DIR"

# Check for huggingface-cli
if ! command -v huggingface-cli &> /dev/null; then
    echo "Installing huggingface_hub..."
    pip install -U huggingface_hub --quiet
fi

download_model() {
    local name="$1"
    local repo="$2"
    local file="$3"
    local size="$4"
    local license="$5"

    local target="$MODELS_DIR/$file"

    if [[ -f "$target" ]]; then
        echo "  [SKIP] $file already exists"
        return
    fi

    echo "  Downloading: $name"
    echo "  Size: $size | License: $license"

    huggingface-cli download "$repo" "$file" \
        --local-dir "$MODELS_DIR" \
        --local-dir-use-symlinks False

    if [[ -f "$target" ]]; then
        echo "  [OK] Downloaded successfully"
    else
        echo "  [ERROR] Download failed"
        exit 1
    fi
}

echo "Tier: $TIER"
echo "Target: $MODELS_DIR"
echo ""

case "$TIER" in
    ci)
        download_model \
            "Qwen 2.5 0.5B (CI/Testing)" \
            "Qwen/Qwen2.5-0.5B-Instruct-GGUF" \
            "qwen2.5-0.5b-instruct-q4_k_m.gguf" \
            "491 MB" \
            "Apache 2.0"
        ;;
    default)
        download_model \
            "Qwen 2.5 1.5B (Default)" \
            "Qwen/Qwen2.5-1.5B-Instruct-GGUF" \
            "qwen2.5-1.5b-instruct-q4_k_m.gguf" \
            "1.1 GB" \
            "Apache 2.0"
        ;;
    quality)
        download_model \
            "Phi-3 Mini (Quality)" \
            "microsoft/Phi-3-mini-4k-instruct-gguf" \
            "Phi-3-mini-4k-instruct-q4.gguf" \
            "2.2 GB" \
            "MIT"
        ;;
    all)
        download_model \
            "Qwen 2.5 0.5B (CI/Testing)" \
            "Qwen/Qwen2.5-0.5B-Instruct-GGUF" \
            "qwen2.5-0.5b-instruct-q4_k_m.gguf" \
            "491 MB" \
            "Apache 2.0"
        echo ""
        download_model \
            "Qwen 2.5 1.5B (Default)" \
            "Qwen/Qwen2.5-1.5B-Instruct-GGUF" \
            "qwen2.5-1.5b-instruct-q4_k_m.gguf" \
            "1.1 GB" \
            "Apache 2.0"
        echo ""
        download_model \
            "Phi-3 Mini (Quality)" \
            "microsoft/Phi-3-mini-4k-instruct-gguf" \
            "Phi-3-mini-4k-instruct-q4.gguf" \
            "2.2 GB" \
            "MIT"
        ;;
    *)
        echo "Usage: $0 [ci|default|quality|all] [models_dir]"
        exit 1
        ;;
esac

echo ""
echo "Done! Models ready in: $MODELS_DIR"
echo ""
echo "Register models with:"
echo "  veritas-sdr-cli model register --name <name> --path models/<file>.gguf --format gguf"
