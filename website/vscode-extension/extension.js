const vscode = require("vscode");
const { execSync, spawn } = require("child_process");

const REFRESH_INTERVAL_MS = 5000;

/**
 * Parse the output of `mhost list` into an array of process objects.
 * Each process has: name, status, pid, uptime.
 */
function parseProcessList(rawOutput) {
  const lines = rawOutput.trim().split("\n").filter((line) => line.length > 0);

  // Skip header line(s) if present
  const dataLines = lines.filter(
    (line) =>
      !line.startsWith("NAME") &&
      !line.startsWith("---") &&
      !line.startsWith("=")
  );

  return dataLines.map((line) => {
    const parts = line.split(/\s{2,}|\t/).map((p) => p.trim());
    return {
      name: parts[0] || "unknown",
      status: (parts[1] || "stopped").toLowerCase(),
      pid: parts[2] || "-",
      uptime: parts[3] || "-",
    };
  });
}

/**
 * Fetch the current process list from mhost CLI.
 * Returns an empty array if mhost is not installed or fails.
 */
function fetchProcesses() {
  try {
    const output = execSync("mhost list", {
      encoding: "utf-8",
      timeout: 10000,
    });
    return parseProcessList(output);
  } catch {
    return [];
  }
}

/**
 * Run an mhost command and show the result.
 */
function runCommand(command, args = []) {
  try {
    const fullCommand = ["mhost", command, ...args].join(" ");
    const output = execSync(fullCommand, { encoding: "utf-8", timeout: 30000 });
    vscode.window.showInformationMessage(`mhost ${command}: ${output.trim()}`);
    return output;
  } catch (error) {
    const message = error.stderr
      ? error.stderr.toString().trim()
      : error.message;
    vscode.window.showErrorMessage(`mhost ${command} failed: ${message}`);
    return null;
  }
}

/**
 * TreeItem representing a single mhost process.
 */
class ProcessTreeItem extends vscode.TreeItem {
  constructor(process) {
    super(process.name, vscode.TreeItemCollapsibleState.None);

    const isOnline = process.status === "online" || process.status === "running";
    const statusIcon = isOnline ? "\u{1F7E2}" : "\u{1F534}";
    const statusLabel = isOnline ? "online" : "stopped";

    this.description = `${statusLabel} | PID: ${process.pid} | Uptime: ${process.uptime}`;
    this.tooltip = `${process.name}\nStatus: ${statusLabel}\nPID: ${process.pid}\nUptime: ${process.uptime}`;
    this.iconPath = new vscode.ThemeIcon(
      isOnline ? "pass" : "error",
      new vscode.ThemeColor(
        isOnline ? "testing.iconPassed" : "testing.iconFailed"
      )
    );
    this.contextValue = isOnline ? "processOnline" : "processStopped";

    this.processData = { ...process, isOnline };
  }
}

/**
 * TreeDataProvider for mhost processes.
 */
class ProcessTreeDataProvider {
  constructor() {
    this._onDidChangeTreeData = new vscode.EventEmitter();
    this.onDidChangeTreeData = this._onDidChangeTreeData.event;
    this._processes = [];
  }

  refresh() {
    this._processes = fetchProcesses();
    this._onDidChangeTreeData.fire();
  }

  getOnlineCount() {
    return this._processes.filter(
      (p) => p.status === "online" || p.status === "running"
    ).length;
  }

  getTotalCount() {
    return this._processes.length;
  }

  getTreeItem(element) {
    return element;
  }

  getChildren() {
    this._processes = fetchProcesses();

    if (this._processes.length === 0) {
      return [createPlaceholderItem()];
    }

    return this._processes.map((proc) => new ProcessTreeItem(proc));
  }
}

function createPlaceholderItem() {
  const item = new vscode.TreeItem(
    "No processes found",
    vscode.TreeItemCollapsibleState.None
  );
  item.description = "Run mhost to start processes";
  item.iconPath = new vscode.ThemeIcon("info");
  return item;
}

/**
 * Stream logs for a process into a VS Code Output Channel.
 */
function streamLogs(processName, outputChannels) {
  const channelName = `mhost: ${processName}`;

  // Reuse existing channel if open
  if (outputChannels.has(channelName)) {
    const existing = outputChannels.get(channelName);
    existing.channel.show(true);
    return;
  }

  const channel = vscode.window.createOutputChannel(channelName);
  channel.show(true);

  const child = spawn("mhost", ["logs", processName, "--follow"], {
    shell: true,
  });

  child.stdout.on("data", (data) => {
    channel.append(data.toString());
  });

  child.stderr.on("data", (data) => {
    channel.append(data.toString());
  });

  child.on("close", (code) => {
    channel.appendLine(`\n--- Log stream ended (exit code: ${code}) ---`);
    outputChannels.delete(channelName);
  });

  outputChannels.set(channelName, { channel, child });
}

/**
 * Activate the mhost VS Code extension.
 */
function activate(context) {
  const treeDataProvider = new ProcessTreeDataProvider();
  const outputChannels = new Map();

  // Register tree view
  const treeView = vscode.window.createTreeView("mhostProcesses", {
    treeDataProvider,
    showCollapseAll: false,
  });

  // Status bar item
  const statusBarItem = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Left,
    100
  );
  statusBarItem.command = "mhost.refresh";
  statusBarItem.show();

  function updateStatusBar() {
    const online = treeDataProvider.getOnlineCount();
    const total = treeDataProvider.getTotalCount();
    statusBarItem.text = `$(pulse) mhost: ${online} online / ${total} total`;
    statusBarItem.tooltip = "Click to refresh mhost processes";
  }

  // Refresh command
  const refreshCmd = vscode.commands.registerCommand("mhost.refresh", () => {
    treeDataProvider.refresh();
    updateStatusBar();
  });

  // Start command
  const startCmd = vscode.commands.registerCommand(
    "mhost.start",
    async (item) => {
      const name = item
        ? item.processData.name
        : await vscode.window.showInputBox({
            prompt: "Process name to start",
          });
      if (name) {
        runCommand("start", [name]);
        treeDataProvider.refresh();
        updateStatusBar();
      }
    }
  );

  // Stop command
  const stopCmd = vscode.commands.registerCommand(
    "mhost.stop",
    async (item) => {
      const name = item
        ? item.processData.name
        : await vscode.window.showInputBox({
            prompt: "Process name to stop",
          });
      if (name) {
        runCommand("stop", [name]);
        treeDataProvider.refresh();
        updateStatusBar();
      }
    }
  );

  // Restart command
  const restartCmd = vscode.commands.registerCommand(
    "mhost.restart",
    async (item) => {
      const name = item
        ? item.processData.name
        : await vscode.window.showInputBox({
            prompt: "Process name to restart",
          });
      if (name) {
        runCommand("restart", [name]);
        treeDataProvider.refresh();
        updateStatusBar();
      }
    }
  );

  // Logs command
  const logsCmd = vscode.commands.registerCommand(
    "mhost.logs",
    async (item) => {
      const name = item
        ? item.processData.name
        : await vscode.window.showInputBox({
            prompt: "Process name for logs",
          });
      if (name) {
        streamLogs(name, outputChannels);
      }
    }
  );

  // Scale command
  const scaleCmd = vscode.commands.registerCommand(
    "mhost.scale",
    async (item) => {
      const name = item
        ? item.processData.name
        : await vscode.window.showInputBox({
            prompt: "Process name to scale",
          });
      if (!name) {
        return;
      }

      const countStr = await vscode.window.showInputBox({
        prompt: `Number of instances for ${name}`,
        validateInput: (value) => {
          const num = parseInt(value, 10);
          if (isNaN(num) || num < 1) {
            return "Please enter a positive integer";
          }
          return null;
        },
      });

      if (countStr) {
        runCommand("scale", [name, countStr]);
        treeDataProvider.refresh();
        updateStatusBar();
      }
    }
  );

  // Auto-refresh interval
  const refreshInterval = setInterval(() => {
    treeDataProvider.refresh();
    updateStatusBar();
  }, REFRESH_INTERVAL_MS);

  // Initial load
  treeDataProvider.refresh();
  updateStatusBar();

  // Register disposables
  context.subscriptions.push(
    treeView,
    statusBarItem,
    refreshCmd,
    startCmd,
    stopCmd,
    restartCmd,
    logsCmd,
    scaleCmd,
    { dispose: () => clearInterval(refreshInterval) },
    {
      dispose: () => {
        for (const { channel, child } of outputChannels.values()) {
          child.kill();
          channel.dispose();
        }
        outputChannels.clear();
      },
    }
  );
}

function deactivate() {}

module.exports = { activate, deactivate };
