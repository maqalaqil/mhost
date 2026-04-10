# mhost - VS Code Extension

Manage your processes with [mhost](https://mhostai.com) directly from VS Code.

## Features

- **Process Tree View** - See all mhost processes in the Activity Bar with live status (online/stopped), PID, and uptime.
- **Context Menu Actions** - Right-click any process to Start, Stop, Restart, Scale, or view Logs.
- **Live Log Streaming** - Open an Output Channel that streams `mhost logs --follow` in real time.
- **Status Bar** - Always-visible indicator showing how many processes are online vs total.
- **Auto-Refresh** - Process list updates every 5 seconds automatically.
- **Scale Command** - Prompts for instance count to scale processes up or down.

## Screenshot

![mhost VS Code Extension](screenshot-placeholder.png)

## Requirements

- **mhost CLI** must be installed and available in your PATH. Install it from [mhostai.com](https://mhostai.com).
- VS Code **1.85.0** or later.

## Installation

1. Download the `.vsix` file from the [releases page](https://github.com/maqalaqil/mhost/releases).
2. In VS Code, open the Command Palette (`Cmd+Shift+P` / `Ctrl+Shift+P`).
3. Run **Extensions: Install from VSIX...** and select the downloaded file.

Or install from the VS Code Marketplace once published:

```
ext install maqalaqil.mhost
```

## Commands

| Command | Description |
|---------|-------------|
| `mhost: Refresh` | Manually refresh the process list |
| `mhost: Start Process` | Start a stopped process |
| `mhost: Stop Process` | Stop a running process |
| `mhost: Restart Process` | Restart a process |
| `mhost: Show Logs` | Stream logs in an Output Channel |
| `mhost: Scale Process` | Change the number of instances |

## Configuration

No additional configuration is needed. The extension calls the `mhost` CLI directly.

## License

MIT
