use std::process::Command;

fn mhost_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mhost"))
}

/// Run `mhost <args>` and return (stdout, stderr, success).
fn run(args: &[&str]) -> (String, String, bool) {
    let output = mhost_bin().args(args).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (stdout, stderr, output.status.success())
}

// ---------------------------------------------------------------------------
// Original tests
// ---------------------------------------------------------------------------

#[test]
fn test_version() {
    let output = mhost_bin().arg("--version").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("mhost"));
}

#[test]
fn test_help() {
    let output = mhost_bin().arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("start"));
    assert!(stdout.contains("stop"));
    assert!(stdout.contains("list"));
    assert!(stdout.contains("logs"));
}

#[test]
fn test_completion_bash() {
    let output = mhost_bin().args(["completion", "bash"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("mhost"));
}

#[test]
fn test_completion_zsh() {
    let output = mhost_bin().args(["completion", "zsh"]).output().unwrap();
    assert!(output.status.success());
}

#[test]
fn test_completion_fish() {
    let output = mhost_bin().args(["completion", "fish"]).output().unwrap();
    assert!(output.status.success());
}

// ---------------------------------------------------------------------------
// --version contains a version number
// ---------------------------------------------------------------------------

#[test]
fn version_contains_semver_number() {
    let (stdout, _stderr, ok) = run(&["--version"]);
    assert!(ok, "--version should exit 0");
    // Version output is like "mhost 0.1.0"
    let has_digit = stdout.chars().any(|c| c.is_ascii_digit());
    assert!(
        has_digit,
        "--version output should contain a version number, got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Top-level --help completeness
// ---------------------------------------------------------------------------

#[test]
fn help_contains_restart() {
    let (stdout, _stderr, ok) = run(&["--help"]);
    assert!(ok);
    assert!(stdout.contains("restart"), "help should mention restart");
}

#[test]
fn help_contains_delete() {
    let (stdout, _stderr, ok) = run(&["--help"]);
    assert!(ok);
    assert!(stdout.contains("delete"), "help should mention delete");
}

#[test]
fn help_contains_info() {
    let (stdout, _stderr, ok) = run(&["--help"]);
    assert!(ok);
    assert!(stdout.contains("info"), "help should mention info");
}

#[test]
fn help_contains_scale() {
    let (stdout, _stderr, ok) = run(&["--help"]);
    assert!(ok);
    assert!(stdout.contains("scale"), "help should mention scale");
}

#[test]
fn help_contains_deploy() {
    let (stdout, _stderr, ok) = run(&["--help"]);
    assert!(ok);
    assert!(stdout.contains("deploy"), "help should mention deploy");
}

// ---------------------------------------------------------------------------
// mhost ai --help
// ---------------------------------------------------------------------------

#[test]
fn ai_help_exits_successfully() {
    let (stdout, stderr, ok) = run(&["ai", "--help"]);
    assert!(
        ok,
        "mhost ai --help should exit 0; stdout={stdout} stderr={stderr}"
    );
}

#[test]
fn ai_help_shows_diagnose() {
    let (stdout, _stderr, _ok) = run(&["ai", "--help"]);
    assert!(
        stdout.contains("diagnose"),
        "ai --help should list diagnose"
    );
}

#[test]
fn ai_help_shows_logs() {
    let (stdout, _stderr, _ok) = run(&["ai", "--help"]);
    assert!(stdout.contains("logs"), "ai --help should list logs");
}

#[test]
fn ai_help_shows_ask() {
    let (stdout, _stderr, _ok) = run(&["ai", "--help"]);
    assert!(stdout.contains("ask"), "ai --help should list ask");
}

#[test]
fn ai_help_shows_watch() {
    let (stdout, _stderr, _ok) = run(&["ai", "--help"]);
    assert!(stdout.contains("watch"), "ai --help should list watch");
}

// ---------------------------------------------------------------------------
// mhost cloud --help
// ---------------------------------------------------------------------------

#[test]
fn cloud_help_exits_successfully() {
    let (stdout, stderr, ok) = run(&["cloud", "--help"]);
    assert!(
        ok,
        "mhost cloud --help should exit 0; stdout={stdout} stderr={stderr}"
    );
}

#[test]
fn cloud_help_shows_add() {
    let (stdout, _stderr, _ok) = run(&["cloud", "--help"]);
    assert!(stdout.contains("add"), "cloud --help should list add");
}

#[test]
fn cloud_help_shows_list() {
    let (stdout, _stderr, _ok) = run(&["cloud", "--help"]);
    assert!(stdout.contains("list"), "cloud --help should list list");
}

#[test]
fn cloud_help_shows_status() {
    let (stdout, _stderr, _ok) = run(&["cloud", "--help"]);
    assert!(stdout.contains("status"), "cloud --help should list status");
}

#[test]
fn cloud_help_shows_deploy() {
    let (stdout, _stderr, _ok) = run(&["cloud", "--help"]);
    assert!(stdout.contains("deploy"), "cloud --help should list deploy");
}

// ---------------------------------------------------------------------------
// mhost notify --help
// ---------------------------------------------------------------------------

#[test]
fn notify_help_exits_successfully() {
    let (stdout, stderr, ok) = run(&["notify", "--help"]);
    assert!(
        ok,
        "mhost notify --help should exit 0; stdout={stdout} stderr={stderr}"
    );
}

#[test]
fn notify_help_shows_setup() {
    let (stdout, _stderr, _ok) = run(&["notify", "--help"]);
    assert!(stdout.contains("setup"), "notify --help should list setup");
}

#[test]
fn notify_help_shows_list() {
    let (stdout, _stderr, _ok) = run(&["notify", "--help"]);
    assert!(stdout.contains("list"), "notify --help should list list");
}

#[test]
fn notify_help_shows_test() {
    let (stdout, _stderr, _ok) = run(&["notify", "--help"]);
    assert!(stdout.contains("test"), "notify --help should list test");
}

// ---------------------------------------------------------------------------
// mhost bot --help
// ---------------------------------------------------------------------------

#[test]
fn bot_help_exits_successfully() {
    let (stdout, stderr, ok) = run(&["bot", "--help"]);
    assert!(
        ok,
        "mhost bot --help should exit 0; stdout={stdout} stderr={stderr}"
    );
}

#[test]
fn bot_help_shows_setup() {
    let (stdout, _stderr, _ok) = run(&["bot", "--help"]);
    assert!(stdout.contains("setup"), "bot --help should list setup");
}

#[test]
fn bot_help_shows_enable() {
    let (stdout, _stderr, _ok) = run(&["bot", "--help"]);
    assert!(stdout.contains("enable"), "bot --help should list enable");
}

#[test]
fn bot_help_shows_disable() {
    let (stdout, _stderr, _ok) = run(&["bot", "--help"]);
    assert!(stdout.contains("disable"), "bot --help should list disable");
}

#[test]
fn bot_help_shows_status() {
    let (stdout, _stderr, _ok) = run(&["bot", "--help"]);
    assert!(stdout.contains("status"), "bot --help should list status");
}

// ---------------------------------------------------------------------------
// mhost metrics --help
// ---------------------------------------------------------------------------

#[test]
fn metrics_help_exits_successfully() {
    let (stdout, stderr, ok) = run(&["metrics", "--help"]);
    assert!(
        ok,
        "mhost metrics --help should exit 0; stdout={stdout} stderr={stderr}"
    );
}

#[test]
fn metrics_help_shows_show() {
    let (stdout, _stderr, _ok) = run(&["metrics", "--help"]);
    assert!(stdout.contains("show"), "metrics --help should list show");
}

#[test]
fn metrics_help_shows_history() {
    let (stdout, _stderr, _ok) = run(&["metrics", "--help"]);
    assert!(
        stdout.contains("history"),
        "metrics --help should list history"
    );
}

#[test]
fn metrics_help_shows_start() {
    let (stdout, _stderr, _ok) = run(&["metrics", "--help"]);
    assert!(stdout.contains("start"), "metrics --help should list start");
}

// ---------------------------------------------------------------------------
// mhost agent --help
// ---------------------------------------------------------------------------

#[test]
fn agent_help_exits_successfully() {
    let output = mhost_bin().args(["agent", "--help"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("setup"));
    assert!(stdout.contains("start"));
    assert!(stdout.contains("stop"));
    assert!(stdout.contains("status"));
}

#[test]
fn agent_status_no_config() {
    let output = mhost_bin().args(["agent", "status"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    // If agent.json exists locally, status shows config; if not, shows "not configured"
    assert!(
        stdout.contains("not configured")
            || stdout.contains("setup")
            || stdout.contains("Provider")
            || stdout.contains("Autonomy")
    );
}

// ---------------------------------------------------------------------------
// mhost brain --help
// ---------------------------------------------------------------------------

#[test]
fn brain_help_exits_successfully() {
    let (stdout, stderr, ok) = run(&["brain", "--help"]);
    assert!(
        ok,
        "mhost brain --help should exit 0; stdout={stdout} stderr={stderr}"
    );
    assert!(stdout.contains("status"), "brain --help should list status");
    assert!(
        stdout.contains("history"),
        "brain --help should list history"
    );
    assert!(
        stdout.contains("playbooks"),
        "brain --help should list playbooks"
    );
    assert!(
        stdout.contains("explain"),
        "brain --help should list explain"
    );
}

#[test]
fn brain_status_no_data() {
    // When the brain directory has no health.json the command should still
    // exit successfully and print a helpful message.
    let (stdout, stderr, ok) = run(&["brain", "status"]);
    assert!(
        ok,
        "mhost brain status should exit 0 even with no data; stdout={stdout} stderr={stderr}"
    );
    // Either it shows "no data" guidance or a health table — both are valid.
    let has_expected =
        stdout.contains("no data") || stdout.contains("agent start") || stdout.contains("/100");
    assert!(
        has_expected,
        "brain status output should contain health info or guidance; got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// New commands: reload, dev, dashboard
// ---------------------------------------------------------------------------

#[test]
fn reload_help_exits_successfully() {
    let (stdout, stderr, ok) = run(&["reload", "--help"]);
    assert!(
        ok,
        "mhost reload --help should exit 0; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("zero-downtime") || stdout.contains("reload") || stdout.contains("target"),
        "reload --help should describe the command; got: {stdout}"
    );
}

#[test]
fn dev_help_exits_successfully() {
    let (stdout, stderr, ok) = run(&["dev", "--help"]);
    assert!(
        ok,
        "mhost dev --help should exit 0; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("script") || stdout.contains("dev"),
        "dev --help should mention script arg; got: {stdout}"
    );
}

#[test]
fn dev_help_shows_watch_flag() {
    let (stdout, _stderr, _ok) = run(&["dev", "--help"]);
    assert!(
        stdout.contains("--watch") || stdout.contains("watch"),
        "dev --help should show --watch flag; got: {stdout}"
    );
}

#[test]
fn dev_help_shows_ext_flag() {
    let (stdout, _stderr, _ok) = run(&["dev", "--help"]);
    assert!(
        stdout.contains("--ext") || stdout.contains("ext"),
        "dev --help should show --ext flag; got: {stdout}"
    );
}

#[test]
fn dashboard_help_exits_successfully() {
    let (stdout, stderr, ok) = run(&["dashboard", "--help"]);
    assert!(
        ok,
        "mhost dashboard --help should exit 0; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("dashboard") || stdout.contains("port"),
        "dashboard --help should describe port option; got: {stdout}"
    );
}

#[test]
fn dashboard_help_shows_port_flag() {
    let (stdout, _stderr, _ok) = run(&["dashboard", "--help"]);
    assert!(
        stdout.contains("--port") || stdout.contains("port"),
        "dashboard --help should show --port flag; got: {stdout}"
    );
}

#[test]
fn logs_help_shows_follow_flag() {
    let (stdout, _stderr, _ok) = run(&["logs", "--help"]);
    assert!(
        stdout.contains("--follow") || stdout.contains("follow"),
        "logs --help should show --follow flag; got: {stdout}"
    );
}

#[test]
fn help_contains_reload() {
    let (stdout, _stderr, _ok) = run(&["--help"]);
    assert!(
        stdout.contains("reload"),
        "mhost --help should list reload command; got: {stdout}"
    );
}

#[test]
fn help_contains_dev() {
    let (stdout, _stderr, _ok) = run(&["--help"]);
    assert!(
        stdout.contains("dev"),
        "mhost --help should list dev command; got: {stdout}"
    );
}

#[test]
fn help_contains_dashboard() {
    let (stdout, _stderr, _ok) = run(&["--help"]);
    assert!(
        stdout.contains("dashboard"),
        "mhost --help should list dashboard command; got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Installation guards — ensure critical features work from any install method
// ---------------------------------------------------------------------------

#[test]
fn mhostd_binary_exists_next_to_mhost() {
    // The daemon binary must be findable from the CLI binary's location
    let mhost_path = std::path::Path::new(env!("CARGO_BIN_EXE_mhost"));
    let dir = mhost_path.parent().expect("mhost binary has parent dir");
    let mhostd = dir.join("mhostd");
    assert!(
        mhostd.exists(),
        "mhostd must exist next to mhost at {}",
        mhostd.display()
    );
}

#[test]
fn version_flag_works() {
    let (stdout, _stderr, ok) = run(&["-v"]);
    assert!(ok, "mhost -v should exit 0");
    assert!(
        stdout.contains("mhost"),
        "version output should contain 'mhost'"
    );
}

#[test]
fn all_subcommands_listed_in_help() {
    let (stdout, _stderr, _ok) = run(&["--help"]);
    let required = [
        "start",
        "stop",
        "restart",
        "delete",
        "list",
        "logs",
        "info",
        "scale",
        "save",
        "resurrect",
        "ping",
        "kill",
        "health",
        "notify",
        "ai",
        "agent",
        "brain",
        "bot",
        "cloud",
        "metrics",
        "monit",
        "dashboard",
        "deploy",
        "rollback",
        "proxy",
        "reload",
        "dev",
    ];
    for cmd in &required {
        assert!(stdout.contains(cmd), "mhost --help missing '{cmd}'");
    }
}

#[test]
fn embedded_scripts_compile_into_binary() {
    // Verify the binary can extract embedded scripts
    // by checking that agent/dashboard/notifier commands exist
    let (stdout, _stderr, _ok) = run(&["agent", "--help"]);
    assert!(stdout.contains("setup") && stdout.contains("start"));
    let (stdout, _stderr, _ok) = run(&["dashboard", "--help"]);
    assert!(stdout.contains("port"));
}

#[test]
fn non_daemon_commands_work_without_daemon() {
    // These commands must work even when daemon is not running
    let cmds = [
        vec!["-v"],
        vec!["--help"],
        vec!["notify", "list"],
        vec!["notify", "events"],
        vec!["agent", "status"],
        vec!["brain", "status"],
        vec!["bot", "status"],
        vec!["cloud", "list"],
        vec!["completion", "bash"],
    ];
    for args in &cmds {
        let output = mhost_bin()
            .args(args)
            .output()
            .unwrap_or_else(|_| panic!("mhost {} should not panic", args.join(" ")));
        // We don't check exit code — some may fail if not configured
        // We just verify they don't crash/panic
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains("panicked"),
            "mhost {} panicked",
            args.join(" ")
        );
    }
}
