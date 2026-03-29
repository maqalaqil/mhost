# mhost

Advanced process manager — PM2 replacement written in Rust.

Single binary, zero dependencies, cross-platform (macOS, Linux, Windows).

## Install

### Homebrew

```bash
brew install maheralaqil/tap/mhost
```

### npm

```bash
npm install -g mhost
```

### Cargo

```bash
cargo install mhost
```

### curl

```bash
curl -fsSL https://mhost.dev/install.sh | sh
```

### PowerShell

```powershell
irm https://mhost.dev/install.ps1 | iex
```

## Quick Start

### Start a process

```bash
mhost start "node server.js" --name api
mhost start "python app.py" --name worker
```

### Start from config file

```bash
mhost start mhost.toml
```

### List all processes

```bash
mhost list
```

### View logs

```bash
mhost logs api -n 100
mhost logs api --grep "error"
mhost logs api --err
```

### Stop a process

```bash
mhost stop api
mhost stop all
```

### Restart a process

```bash
mhost restart api
mhost restart all
```

### Scale a process

```bash
mhost scale api 4
```

### Process information

```bash
mhost info api
mhost env api
mhost history api
mhost config api
```

### Daemon management

```bash
mhost ping
mhost kill
```

### Persistence

```bash
mhost save
mhost resurrect
```

## Ecosystem Config

Create an `mhost.toml` file to manage multiple processes:

```toml
[app]
command = "node"
args = ["server.js"]
cwd = "/path/to/app"
instances = 1
max_restarts = 15
min_uptime = "10s"
restart_delay = "5s"
grace_period = "30s"
max_memory = "512M"

[app.env]
NODE_ENV = "production"
LOG_LEVEL = "info"

[worker]
command = "python"
args = ["worker.py"]
instances = 4
max_restarts = 10
min_uptime = "5s"
restart_delay = "2s"
max_memory = "256M"

[worker.env]
PYTHONUNBUFFERED = "1"

[scheduler]
command = "/usr/local/bin/scheduler"
cron_restart = "0 2 * * *"
```

Start all processes with:

```bash
mhost start mhost.toml
```

## Commands

| Command | Description |
|---------|-------------|
| `start <target> [--name NAME]` | Start a process or load ecosystem config (TOML/YAML/JSON) |
| `stop <name \| all>` | Stop a running process or all processes |
| `restart <name \| all>` | Restart a process or all processes |
| `delete <name \| all>` | Remove a process from the registry |
| `list` (or `ls`) | List all managed processes |
| `logs <name> [-n LINES] [--err] [--grep PATTERN]` | Tail log output for a process |
| `info <name>` | Show detailed information about a process |
| `env <name>` | Print environment variables for a process |
| `scale <name> <instances>` | Scale a process to a specific number of instances |
| `save` | Save the current process list for resurrection on next startup |
| `resurrect` | Restore all previously saved processes |
| `ping` | Ping the daemon |
| `kill` | Kill the daemon |
| `history <name>` | Show event history for a process |
| `config <name>` | Print the configuration for a process as JSON |
| `startup` | Generate a startup script to launch mhost at login/boot |
| `unstartup` | Remove the startup script |
| `self-update` | Check for a newer mhost release and update if available |
| `completion <shell>` | Generate shell completion scripts (bash, zsh, fish, powershell) |

## License

MIT
