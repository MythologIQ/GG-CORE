# Veritas SDR Model Download Script
# Downloads recommended GGUF models for testing and production

param(
    [ValidateSet("ci", "default", "quality", "all")]
    [string]$Tier = "default",
    [string]$ModelsDir = "models"
)

$ErrorActionPreference = "Stop"

# Ensure models directory exists
if (-not (Test-Path $ModelsDir)) {
    New-Item -ItemType Directory -Path $ModelsDir -Force | Out-Null
}

Write-Host "Veritas SDR Model Downloader" -ForegroundColor Cyan
Write-Host "=============================" -ForegroundColor Cyan

# Model definitions
$Models = @{
    "ci" = @{
        Name = "Qwen 2.5 0.5B (CI/Testing)"
        Repo = "Qwen/Qwen2.5-0.5B-Instruct-GGUF"
        File = "qwen2.5-0.5b-instruct-q4_k_m.gguf"
        Size = "491 MB"
        License = "Apache 2.0"
    }
    "default" = @{
        Name = "Qwen 2.5 1.5B (Default)"
        Repo = "Qwen/Qwen2.5-1.5B-Instruct-GGUF"
        File = "qwen2.5-1.5b-instruct-q4_k_m.gguf"
        Size = "1.1 GB"
        License = "Apache 2.0"
    }
    "quality" = @{
        Name = "Phi-3 Mini (Quality)"
        Repo = "microsoft/Phi-3-mini-4k-instruct-gguf"
        File = "Phi-3-mini-4k-instruct-q4.gguf"
        Size = "2.2 GB"
        License = "MIT"
    }
}

function Download-Model {
    param([hashtable]$Model)

    $targetPath = Join-Path $ModelsDir $Model.File

    if (Test-Path $targetPath) {
        Write-Host "  [SKIP] $($Model.File) already exists" -ForegroundColor Yellow
        return
    }

    Write-Host "  Downloading: $($Model.Name)" -ForegroundColor Green
    Write-Host "  Size: $($Model.Size) | License: $($Model.License)"

    # Check for huggingface-cli
    $hfCli = Get-Command huggingface-cli -ErrorAction SilentlyContinue
    if (-not $hfCli) {
        Write-Host "  Installing huggingface_hub..." -ForegroundColor Yellow
        pip install -U huggingface_hub --quiet
    }

    huggingface-cli download $Model.Repo $Model.File `
        --local-dir $ModelsDir `
        --local-dir-use-symlinks False

    if (Test-Path $targetPath) {
        Write-Host "  [OK] Downloaded successfully" -ForegroundColor Green
    } else {
        Write-Host "  [ERROR] Download failed" -ForegroundColor Red
        exit 1
    }
}

# Determine which models to download
$toDownload = @()
switch ($Tier) {
    "ci" { $toDownload = @("ci") }
    "default" { $toDownload = @("default") }
    "quality" { $toDownload = @("quality") }
    "all" { $toDownload = @("ci", "default", "quality") }
}

Write-Host ""
Write-Host "Tier: $Tier" -ForegroundColor Cyan
Write-Host "Target: $ModelsDir" -ForegroundColor Cyan
Write-Host ""

foreach ($tier in $toDownload) {
    Download-Model $Models[$tier]
    Write-Host ""
}

Write-Host "Done! Models ready in: $ModelsDir" -ForegroundColor Green
Write-Host ""
Write-Host "Register models with:"
Write-Host "  veritas-sdr-cli model register --name <name> --path models/<file>.gguf --format gguf"
