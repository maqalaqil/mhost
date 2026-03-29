use std::io::{self, Write};

use mhost_ai::{AiConfig, ProcessContext};
use mhost_core::paths::MhostPaths;
use mhost_core::process::ProcessInfo;
use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;

use crate::output::print_success;

// ─── Setup Wizard ───────────────────────────────────────────────────────────

pub fn run_setup(paths: &MhostPaths) -> Result<(), String> {
    println!("\n  mhost AI Setup\n");
    println!("  Select LLM provider:");
    println!("    1) OpenAI (GPT-4o, GPT-4o-mini)");
    println!("    2) Claude (Sonnet, Haiku, Opus)");
    println!();

    let choice = prompt("Provider (1-2)");
    let (provider, default_model) = match choice.as_str() {
        "1" => ("openai", "gpt-4o"),
        "2" => ("claude", "claude-sonnet-4-20250514"),
        _ => return Err("Invalid choice".into()),
    };

    let api_key = prompt("API key");
    if api_key.is_empty() {
        return Err("API key is required".into());
    }

    let model = prompt_default("Model", default_model);

    let config = AiConfig {
        provider: provider.into(),
        api_key,
        model,
        max_tokens: 4096,
    };

    config.save(&paths.ai_config())?;
    print_success(&format!(
        "AI configured with {} ({})",
        provider, config.model
    ));
    println!("  Config: {}", paths.ai_config().display());
    println!("  Test:   mhost ai ask \"what processes are running?\"");
    Ok(())
}

// ─── Diagnose ───────────────────────────────────────────────────────────────

pub async fn run_diagnose(
    client: &IpcClient,
    paths: &MhostPaths,
    name: &str,
) -> Result<(), String> {
    let provider = load_provider(paths)?;
    let context = fetch_process_context(client, paths, name).await?;

    println!("  Analyzing crash for '{}'...\n", name);
    let result = mhost_ai::diagnose::diagnose(provider.as_ref(), &context).await?;
    println!("{}", result);
    Ok(())
}

// ─── AI Log Query ────────────────────────────────────────────────────────────

pub async fn run_log_query(
    _client: &IpcClient,
    paths: &MhostPaths,
    name: &str,
    question: &str,
) -> Result<(), String> {
    let provider = load_provider(paths)?;

    println!("  Translating query: \"{}\"...\n", question);
    let query = mhost_ai::log_query::translate_log_query(provider.as_ref(), name, question).await?;

    println!("  Search: {:?}", query.search);
    println!("  Level:  {:?}", query.level);
    println!("  Since:  {:?}", query.since);
    println!("  Limit:  {:?}", query.limit);
    // In a full implementation, this would execute the query against the log indexer
    Ok(())
}

// ─── Optimize ────────────────────────────────────────────────────────────────

pub async fn run_optimize(
    client: &IpcClient,
    paths: &MhostPaths,
    name: &str,
) -> Result<(), String> {
    let provider = load_provider(paths)?;
    let context = fetch_process_context(client, paths, name).await?;

    println!("  Analyzing performance for '{}'...\n", name);
    let result = mhost_ai::optimize::optimize(
        provider.as_ref(),
        &context,
        "No metrics history available yet",
    )
    .await?;
    println!("{}", result);
    Ok(())
}

// ─── Config Gen ──────────────────────────────────────────────────────────────

pub async fn run_config_gen(paths: &MhostPaths, description: &str) -> Result<(), String> {
    let provider = load_provider(paths)?;

    println!("  Generating config from description...\n");
    let toml = mhost_ai::config_gen::generate_config(provider.as_ref(), description).await?;
    println!("{}", toml);
    println!("\n  To save: copy the above into mhost.toml");
    Ok(())
}

// ─── Postmortem ──────────────────────────────────────────────────────────────

pub async fn run_postmortem(
    client: &IpcClient,
    paths: &MhostPaths,
    name: &str,
) -> Result<(), String> {
    let provider = load_provider(paths)?;
    let context = fetch_process_context(client, paths, name).await?;

    println!("  Generating incident report for '{}'...\n", name);
    let report = mhost_ai::postmortem::generate_postmortem(
        provider.as_ref(),
        &context,
        "No metrics history available",
    )
    .await?;
    println!("{}", report);
    Ok(())
}

// ─── Watch ───────────────────────────────────────────────────────────────────

pub async fn run_watch(client: &IpcClient, paths: &MhostPaths) -> Result<(), String> {
    let provider = load_provider(paths)?;
    let processes = fetch_process_list(client).await?;

    println!(
        "  Scanning {} processes for anomalies...\n",
        processes.len()
    );

    let mut log_batches: Vec<(String, Vec<String>)> = Vec::new();
    for proc in &processes {
        let out_log = paths.process_out_log(&proc.config.name, proc.instance);
        let lines = if out_log.exists() {
            mhost_logs::reader::tail(&out_log, 20).unwrap_or_default()
        } else {
            Vec::new()
        };
        log_batches.push((proc.config.name.clone(), lines));
    }

    let alerts = mhost_ai::watch::detect_anomalies(provider.as_ref(), &log_batches).await?;

    if alerts.is_empty() {
        print_success("No anomalies detected");
    } else {
        for alert in &alerts {
            let icon = match alert.severity.as_str() {
                "critical" => "!!",
                "warning" => "!",
                _ => "i",
            };
            println!("  [{}] {}: {}", icon, alert.process, alert.message);
        }
    }
    Ok(())
}

// ─── Ask ─────────────────────────────────────────────────────────────────────

pub async fn run_ask(client: &IpcClient, paths: &MhostPaths, question: &str) -> Result<(), String> {
    let provider = load_provider(paths)?;
    let processes = fetch_process_list(client).await?;

    let contexts: Vec<ProcessContext> = processes
        .iter()
        .map(|p| ProcessContext::from_process_info(p, Vec::new(), Vec::new(), Vec::new()))
        .collect();

    println!("  Thinking...\n");
    let answer = mhost_ai::ask::ask(provider.as_ref(), question, &contexts).await?;
    println!("{}", answer);
    Ok(())
}

// ─── Explain ─────────────────────────────────────────────────────────────────

pub async fn run_explain(paths: &MhostPaths, config_file: &str) -> Result<(), String> {
    let provider = load_provider(paths)?;
    let content = std::fs::read_to_string(config_file)
        .map_err(|e| format!("Cannot read '{}': {}", config_file, e))?;

    println!("  Explaining config...\n");
    let explanation = mhost_ai::explain::explain_config(provider.as_ref(), &content).await?;
    println!("{}", explanation);
    Ok(())
}

// ─── Suggest ─────────────────────────────────────────────────────────────────

pub async fn run_suggest(client: &IpcClient, paths: &MhostPaths) -> Result<(), String> {
    let provider = load_provider(paths)?;
    let processes = fetch_process_list(client).await?;

    let contexts: Vec<ProcessContext> = processes
        .iter()
        .map(|p| ProcessContext::from_process_info(p, Vec::new(), Vec::new(), Vec::new()))
        .collect();

    println!("  Analyzing {} processes...\n", contexts.len());
    let suggestions = mhost_ai::explain::suggest_improvements(provider.as_ref(), &contexts).await?;
    println!("{}", suggestions);
    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn load_provider(paths: &MhostPaths) -> Result<Box<dyn mhost_ai::LlmProvider>, String> {
    let config =
        AiConfig::load(&paths.ai_config()).ok_or("AI not configured. Run: mhost ai setup")?;
    config.create_provider()
}

fn prompt(label: &str) -> String {
    print!("  {}: ", label);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

fn prompt_default(label: &str, default: &str) -> String {
    print!("  {} [{}]: ", label, default);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let val = input.trim();
    if val.is_empty() {
        default.to_string()
    } else {
        val.to_string()
    }
}

async fn fetch_process_list(client: &IpcClient) -> Result<Vec<ProcessInfo>, String> {
    let resp = client
        .call(methods::PROCESS_LIST, serde_json::json!(null))
        .await
        .map_err(|e| format!("IPC error: {e}"))?;
    let result = resp.result.unwrap_or(serde_json::Value::Array(vec![]));
    let list = if let Some(arr) = result.get("processes") {
        arr.clone()
    } else {
        result
    };
    serde_json::from_value(list).map_err(|e| format!("Parse error: {e}"))
}

async fn fetch_process_context(
    client: &IpcClient,
    paths: &MhostPaths,
    name: &str,
) -> Result<ProcessContext, String> {
    let processes = fetch_process_list(client).await?;
    let info = processes
        .iter()
        .find(|p| p.config.name == name)
        .ok_or_else(|| format!("Process '{}' not found", name))?;

    let out_log = paths.process_out_log(name, info.instance);
    let err_log = paths.process_err_log(name, info.instance);
    let recent_logs = if out_log.exists() {
        mhost_logs::reader::tail(&out_log, 50).unwrap_or_default()
    } else {
        Vec::new()
    };
    let error_logs = if err_log.exists() {
        mhost_logs::reader::tail(&err_log, 20).unwrap_or_default()
    } else {
        Vec::new()
    };

    Ok(ProcessContext::from_process_info(
        info,
        recent_logs,
        error_logs,
        Vec::new(),
    ))
}
