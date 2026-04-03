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
    console.log('Installing mhost binaries...');

    fs.mkdirSync(VENDOR_DIR, { recursive: true });

    const target = getPlatform();
    console.log(`Platform: ${target}`);

    // Get latest version from GitHub API
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

    // Download
    console.log(`Downloading ${archiveName}...`);
    try {
      execSync(`curl -fsSL -o "${archivePath}" "${url}"`, {
        stdio: ['pipe', 'pipe', 'pipe'],
        timeout: 120000,
      });
    } catch (e) {
      throw new Error(`Download failed from ${url}`);
    }

    // Extract BOTH mhost and mhostd
    console.log('Extracting...');
    if (isWindows) {
      execSync(`tar -xf "${archivePath}" -C "${VENDOR_DIR}"`, { stdio: 'pipe' });
    } else {
      execSync(`tar xzf "${archivePath}" -C "${VENDOR_DIR}"`, { stdio: 'pipe' });
      // Make both binaries executable
      const mhost = path.join(VENDOR_DIR, 'mhost');
      const mhostd = path.join(VENDOR_DIR, 'mhostd');
      if (fs.existsSync(mhost)) fs.chmodSync(mhost, 0o755);
      if (fs.existsSync(mhostd)) fs.chmodSync(mhostd, 0o755);
    }

    // Clean up archive
    try { fs.unlinkSync(archivePath); } catch {}

    // Verify both binaries exist
    const binExt = isWindows ? '.exe' : '';
    const mhostBin = path.join(VENDOR_DIR, `mhost${binExt}`);
    const mhostdBin = path.join(VENDOR_DIR, `mhostd${binExt}`);

    if (!fs.existsSync(mhostBin)) {
      throw new Error(`mhost binary not found after extraction`);
    }
    if (!fs.existsSync(mhostdBin)) {
      console.warn('Warning: mhostd daemon binary not found — some features may not work');
    }

    // Get version
    let ver = '';
    try { ver = require('child_process').execSync(`"${mhostBin}" -v 2>/dev/null`, { encoding: 'utf-8' }).trim(); } catch {}

    console.log('');
    console.log('  ╔══════════════════════════════════════════════════════╗');
    console.log('  ║                                                      ║');
    console.log('  ║   ✔  mhost installed successfully                    ║');
    console.log('  ║                                                      ║');
    console.log(`  ║   ${(ver || 'mhost').padEnd(52)}║`);
    console.log(`  ║   Platform: ${target.padEnd(40)}║`);
    console.log('  ║                                                      ║');
    console.log('  ║   Get started:                                       ║');
    console.log('  ║     mhost start server.js        Start a process     ║');
    console.log('  ║     mhost list                   See what\'s running  ║');
    console.log('  ║     mhost logs <app>             View logs           ║');
    console.log('  ║     mhost --help                 All commands        ║');
    console.log('  ║                                                      ║');
    console.log('  ║   Docs: https://mhostai.com                          ║');
    console.log('  ║                                                      ║');
    console.log('  ╚══════════════════════════════════════════════════════╝');
    console.log('');

  } catch (error) {
    console.error('');
    console.error('  ╔══════════════════════════════════════════════════════╗');
    console.error('  ║  ✖  mhost installation failed                       ║');
    console.error(`  ║  ${error.message.substring(0, 52).padEnd(52)}║`);
    console.error('  ║                                                      ║');
    console.error('  ║  Try: curl -fsSL mhostai.com/install.sh | sh         ║');
    console.error('  ╚══════════════════════════════════════════════════════╝');
    console.error('');
    process.exit(1);
  }
}

install();
