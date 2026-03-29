#!/usr/bin/env node

const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');
const https = require('https');

const REPO = 'maqalaqil/mhost';
const VENDOR_DIR = path.join(__dirname, 'vendor');

const PLATFORM_MAP = {
  'darwin-arm64': 'aarch64-apple-darwin',
  'darwin-x64': 'x86_64-apple-darwin',
  'linux-arm64': 'aarch64-unknown-linux-musl',
  'linux-x64': 'x86_64-unknown-linux-musl',
  'win32-x64': 'x86_64-pc-windows-msvc',
  'win32-arm64': 'aarch64-pc-windows-msvc',
};

function getPlatform() {
  const platform = process.platform;
  const arch = process.arch === 'x64' ? 'x64' : process.arch === 'arm64' ? 'arm64' : process.arch;
  const key = `${platform}-${arch}`;

  if (!PLATFORM_MAP[key]) {
    throw new Error(`Unsupported platform: ${platform}-${arch}`);
  }

  return PLATFORM_MAP[key];
}

function getLatestRelease() {
  return new Promise((resolve, reject) => {
    const options = {
      hostname: 'api.github.com',
      path: `/repos/${REPO}/releases/latest`,
      method: 'GET',
      headers: { 'User-Agent': 'mhost-install' },
    };

    https.request(options, (res) => {
      let data = '';
      res.on('data', (chunk) => { data += chunk; });
      res.on('end', () => {
        try {
          resolve(JSON.parse(data));
        } catch (e) {
          reject(e);
        }
      });
    }).on('error', reject).end();
  });
}

function downloadFile(url, destination) {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(destination);
    https.get(url, (res) => {
      if (res.statusCode !== 200) {
        reject(new Error(`Download failed: ${res.statusCode}`));
      }
      res.pipe(file);
      file.on('finish', () => {
        file.close();
        resolve();
      });
    }).on('error', reject);
  });
}

async function install() {
  try {
    console.log('Installing mhost binary...');

    // Create vendor directory
    if (!fs.existsSync(VENDOR_DIR)) {
      fs.mkdirSync(VENDOR_DIR, { recursive: true });
    }

    // Get platform
    const targetTriple = getPlatform();
    console.log(`Detected platform: ${targetTriple}`);

    // Get latest release
    const release = await getLatestRelease();
    const version = release.tag_name || release.name;
    console.log(`Found version: ${version}`);

    // Find asset for this platform
    const binaryName = process.platform === 'win32' ? 'mhost.exe' : 'mhost';
    const assetName = `mhost-${targetTriple}${process.platform === 'win32' ? '.exe' : ''}`;

    const asset = release.assets.find((a) =>
      a.name.includes(targetTriple) && a.name.includes(binaryName)
    );

    if (!asset) {
      throw new Error(`No binary found for ${targetTriple}`);
    }

    // Download binary
    const binaryPath = path.join(VENDOR_DIR, binaryName);
    console.log(`Downloading ${asset.name}...`);
    await downloadFile(asset.browser_download_url, binaryPath);

    // Make executable (Unix)
    if (process.platform !== 'win32') {
      fs.chmodSync(binaryPath, 0o755);
    }

    console.log('Installation complete!');
  } catch (error) {
    console.error('Installation failed:', error.message);
    process.exit(1);
  }
}

install();
