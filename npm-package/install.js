#!/usr/bin/env node

const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

const REPO = 'maqalaqil/mhost';
const VENDOR_DIR = path.join(__dirname, 'vendor');

const PLATFORM_MAP = {
  'darwin-arm64': 'aarch64-apple-darwin',
  'darwin-x64': 'x86_64-apple-darwin',
  'linux-arm64': 'aarch64-unknown-linux-musl',
  'linux-x64': 'x86_64-unknown-linux-musl',
  'win32-x64': 'x86_64-pc-windows-msvc',
};

function getPlatform() {
  const key = `${process.platform}-${process.arch}`;
  if (!PLATFORM_MAP[key]) {
    console.error(`Unsupported platform: ${key}`);
    console.error(`Supported: ${Object.keys(PLATFORM_MAP).join(', ')}`);
    process.exit(1);
  }
  return PLATFORM_MAP[key];
}

async function install() {
  try {
    console.log('Installing mhost binary...');

    fs.mkdirSync(VENDOR_DIR, { recursive: true });

    const target = getPlatform();
    console.log(`Platform: ${target}`);

    // Get latest version from GitHub API using curl
    let version;
    try {
      const versionOutput = execSync(
        `curl -fsSL https://api.github.com/repos/${REPO}/releases/latest 2>/dev/null`,
        { encoding: 'utf-8', timeout: 15000 }
      );
      const release = JSON.parse(versionOutput);
      version = release.tag_name;
    } catch {
      version = 'v0.1.0';
      console.log(`Could not fetch latest version, using ${version}`);
    }
    console.log(`Version: ${version}`);

    const isWindows = process.platform === 'win32';
    const ext = isWindows ? '.zip' : '.tar.gz';
    const archiveName = `mhost-${target}${ext}`;
    const url = `https://github.com/${REPO}/releases/download/${version}/${archiveName}`;
    const archivePath = path.join(VENDOR_DIR, archiveName);
    const binName = isWindows ? 'mhost.exe' : 'mhost';

    // Download using curl (follows redirects automatically)
    console.log(`Downloading ${archiveName}...`);
    try {
      execSync(`curl -fsSL -o "${archivePath}" "${url}"`, {
        stdio: ['pipe', 'pipe', 'pipe'],
        timeout: 60000,
      });
    } catch (e) {
      throw new Error(`Download failed from ${url}`);
    }

    // Extract
    console.log('Extracting...');
    if (isWindows) {
      execSync(`tar -xf "${archivePath}" -C "${VENDOR_DIR}" ${binName}`, {
        stdio: 'pipe',
      });
    } else {
      execSync(`tar xzf "${archivePath}" -C "${VENDOR_DIR}" ${binName}`, {
        stdio: 'pipe',
      });
      fs.chmodSync(path.join(VENDOR_DIR, binName), 0o755);
    }

    // Clean up archive
    try { fs.unlinkSync(archivePath); } catch {}

    // Verify
    const installed = path.join(VENDOR_DIR, binName);
    if (!fs.existsSync(installed)) {
      throw new Error(`Binary not found after extraction: ${installed}`);
    }

    console.log(`mhost installed to ${installed}`);
  } catch (error) {
    console.error(`Installation failed: ${error.message}`);
    process.exit(1);
  }
}

install();
