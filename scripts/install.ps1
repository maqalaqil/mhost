param(
    [string]$InstallDir = "$env:LOCALAPPDATA\mhost\bin"
)

$ErrorActionPreference = "Stop"

$REPO = "maheralaqil/mhost"
$OS = "windows"

# Detect architecture
$ARCH = if ([System.Environment]::Is64BitOperatingSystem) {
    if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") {
        "aarch64"
    } else {
        "x86_64"
    }
} else {
    Write-Host "Error: 32-bit Windows is not supported" -ForegroundColor Red
    exit 1
}

$TARGET_TRIPLE = switch ("${ARCH}") {
    "x86_64" { "x86_64-pc-windows-msvc" }
    "aarch64" { "aarch64-pc-windows-msvc" }
    default {
        Write-Host "Error: Unsupported architecture: $ARCH" -ForegroundColor Red
        exit 1
    }
}

Write-Host "Detecting latest version..."
try {
    $LATEST_RELEASE = Invoke-RestMethod -Uri "https://api.github.com/repos/$REPO/releases/latest" -UseBasicParsing
    $VERSION = $LATEST_RELEASE.tag_name
} catch {
    Write-Host "Error: Could not detect latest version" -ForegroundColor Red
    exit 1
}

if (-not $VERSION) {
    Write-Host "Error: Could not parse version from API response" -ForegroundColor Red
    exit 1
}

Write-Host "Found version: $VERSION" -ForegroundColor Green

# Download binary
$BINARY_NAME = "mhost-${TARGET_TRIPLE}.exe"
$DOWNLOAD_URL = "https://github.com/$REPO/releases/download/$VERSION/$BINARY_NAME"

Write-Host "Downloading mhost from $DOWNLOAD_URL..."

$TEMP_FILE = New-TemporaryFile

try {
    Invoke-WebRequest -Uri $DOWNLOAD_URL -OutFile $TEMP_FILE -UseBasicParsing -ErrorAction Stop
} catch {
    Write-Host "Error: Failed to download mhost" -ForegroundColor Red
    Remove-Item $TEMP_FILE -ErrorAction SilentlyContinue
    exit 1
}

# Create install directory
if (-not (Test-Path $InstallDir)) {
    Write-Host "Creating directory: $InstallDir"
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

# Install binary
$INSTALL_PATH = Join-Path $InstallDir "mhost.exe"
Write-Host "Installing mhost to $INSTALL_PATH..."

Move-Item -Path $TEMP_FILE -Destination $INSTALL_PATH -Force

# Add to PATH if not already present
$CURRENT_PATH = [System.Environment]::GetEnvironmentVariable("PATH", "User")
if (-not $CURRENT_PATH.Contains($InstallDir)) {
    Write-Host "Adding $InstallDir to PATH..."
    $NEW_PATH = "$CURRENT_PATH;$InstallDir"
    [System.Environment]::SetEnvironmentVariable("PATH", $NEW_PATH, "User")
    Write-Host "PATH updated. Please restart PowerShell for changes to take effect." -ForegroundColor Yellow
} else {
    Write-Host "$InstallDir is already in PATH" -ForegroundColor Green
}

# Verify installation
Write-Host "Verifying installation..."
if (Test-Path $INSTALL_PATH) {
    Write-Host "Installation complete!" -ForegroundColor Green
    Write-Host "mhost installed at: $INSTALL_PATH"
    try {
        & $INSTALL_PATH --version
    } catch {
        Write-Host "Warning: Could not run version check" -ForegroundColor Yellow
    }
} else {
    Write-Host "Error: Installation failed" -ForegroundColor Red
    exit 1
}
