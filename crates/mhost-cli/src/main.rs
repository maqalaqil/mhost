mod cli;
mod commands;
mod daemon_launcher;
mod output;

use clap::Parser;
use mhost_core::paths::MhostPaths;
use mhost_ipc::IpcClient;

use cli::{AiAction, BotAction, Cli, CloudAction, Commands, MetricsAction, NotifyAction};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let paths = MhostPaths::new();

    let result = dispatch(cli, &paths).await;

    if let Err(e) = result {
        output::print_error(&e);
        std::process::exit(1);
    }
}

async fn dispatch(cli: Cli, paths: &MhostPaths) -> Result<(), String> {
    match cli.command {
        // ---- Commands that don't need the daemon -------------------------
        Commands::Startup => commands::startup::run_startup(),
        Commands::Unstartup => commands::startup::run_unstartup(),
        Commands::SelfUpdate => commands::self_update::run(),
        Commands::Completion { shell } => {
            commands::completion::run(shell);
            Ok(())
        }
        Commands::History { name } => commands::history::run(paths, &name),
        Commands::Notify { action } => match action {
            NotifyAction::Setup => commands::notify::run_setup(&paths.notify_config()),
            NotifyAction::List => commands::notify::run_list(&paths.notify_config()),
            NotifyAction::Remove { channel } => {
                commands::notify::run_remove(&paths.notify_config(), &channel)
            }
            NotifyAction::Enable { channel } => {
                commands::notify::run_enable(&paths.notify_config(), &channel, true)
            }
            NotifyAction::Disable { channel } => {
                commands::notify::run_enable(&paths.notify_config(), &channel, false)
            }
            NotifyAction::Events { channel } => {
                commands::notify::run_events(&paths.notify_config(), channel.as_deref())
            }
            NotifyAction::Test { channel } => {
                commands::notify::run_test(&paths.notify_config(), &channel).await
            }
            NotifyAction::Start => {
                daemon_launcher::ensure_daemon_running(paths)?;
                let client = IpcClient::new(&paths.socket());
                commands::notify::run_start(&paths.notify_config(), &client).await
            }
        },
        Commands::Logs {
            name,
            lines,
            err,
            grep,
            search,
            r#where,
            since,
            format,
            count_by,
        } => {
            // --search and --count-by require daemon access
            if search.is_some() || count_by.is_some() {
                let n = name.as_deref().unwrap_or("*");
                daemon_launcher::ensure_daemon_running(paths)?;
                let client = IpcClient::new(&paths.socket());
                if let Some(ref query) = search {
                    commands::logs::search(
                        &client,
                        n,
                        query,
                        r#where.as_deref(),
                        since.as_deref(),
                        &format,
                    )
                    .await
                } else if let Some(ref field) = count_by {
                    commands::logs::count_by(&client, n, field, since.as_deref()).await
                } else {
                    unreachable!()
                }
            } else {
                match name {
                    Some(n) => commands::logs::run(paths, &n, lines, err, grep.as_deref()),
                    None => commands::logs::run_all(paths, lines, err, grep.as_deref()),
                }
            }
        }

        // ---- AI commands that don't need the daemon ----------------------
        Commands::Ai {
            action: AiAction::Setup,
        } => commands::ai::run_setup(paths),
        Commands::Ai {
            action: AiAction::Config { description },
        } => commands::ai::run_config_gen(paths, &description).await,
        Commands::Ai {
            action: AiAction::Explain { file },
        } => commands::ai::run_explain(paths, &file).await,

        // ---- Cloud commands (remote SSH, no local daemon needed) ---------
        Commands::Cloud { action } => dispatch_cloud(action, paths).await,

        // ---- Bot commands (no daemon needed except Enable) ---------------
        Commands::Bot { action } => match action {
            BotAction::Setup => commands::bot::run_setup(paths),
            BotAction::Enable => commands::bot::run_enable(paths).await,
            BotAction::Disable => commands::bot::run_disable(paths),
            BotAction::Status => commands::bot::run_status(paths),
            BotAction::Permissions => commands::bot::run_permissions(paths),
            BotAction::AddAdmin { user_id } => commands::bot::run_add_user(paths, user_id, "admin"),
            BotAction::AddOperator { user_id } => {
                commands::bot::run_add_user(paths, user_id, "operator")
            }
            BotAction::AddViewer { user_id } => {
                commands::bot::run_add_user(paths, user_id, "viewer")
            }
            BotAction::RemoveUser { user_id } => commands::bot::run_remove_user(paths, user_id),
            BotAction::Logs => commands::bot::run_logs(paths),
        },

        // ---- Commands that require a running daemon ----------------------
        other => {
            daemon_launcher::ensure_daemon_running(paths)?;
            let client = IpcClient::new(&paths.socket());
            dispatch_daemon(other, &client, paths).await
        }
    }
}

async fn dispatch_daemon(
    cmd: Commands,
    client: &IpcClient,
    _paths: &MhostPaths,
) -> Result<(), String> {
    match cmd {
        Commands::Start {
            target,
            name,
            group,
        } => {
            if let Some(ref g) = group {
                commands::group::start(client, g).await
            } else {
                commands::start::run(client, &target, name.as_deref()).await
            }
        }
        Commands::Stop { target, group } => {
            if let Some(ref g) = group {
                commands::group::stop(client, g).await
            } else {
                commands::stop::run(client, &target).await
            }
        }
        Commands::Restart { target } => commands::restart::run(client, &target).await,
        Commands::Delete { target } => commands::delete::run(client, &target).await,
        Commands::List => commands::list::run(client).await,
        Commands::Info { name } => commands::info::run(client, &name).await,
        Commands::Env { name } => commands::env_cmd::run(client, &name).await,
        Commands::Scale { name, instances } => commands::scale::run(client, &name, instances).await,
        Commands::Save => commands::save::run(client).await,
        Commands::Resurrect => commands::resurrect::run(client).await,
        Commands::Ping => commands::ping::run(client).await,
        Commands::Kill => commands::kill::run(client).await,
        Commands::Config { name } => commands::config_cmd::run(client, &name).await,
        Commands::Health { name } => commands::health::run(client, &name).await,
        Commands::Cluster { name, instances } => {
            commands::cluster::run(client, &name, instances).await
        }

        Commands::Metrics { action } => match action {
            MetricsAction::Show { name } => commands::metrics::show(client, &name).await,
            MetricsAction::History {
                name,
                metric,
                since,
            } => commands::metrics::history(client, &name, &metric, &since).await,
            MetricsAction::Start { listen } => {
                commands::metrics::start_prometheus(client, &listen).await
            }
        },

        Commands::Ai { action } => match action {
            AiAction::Diagnose { name } => commands::ai::run_diagnose(client, _paths, &name).await,
            AiAction::Logs { name, question } => {
                commands::ai::run_log_query(client, _paths, &name, &question).await
            }
            AiAction::Optimize { name } => commands::ai::run_optimize(client, _paths, &name).await,
            AiAction::Postmortem { name } => {
                commands::ai::run_postmortem(client, _paths, &name).await
            }
            AiAction::Watch => commands::ai::run_watch(client, _paths).await,
            AiAction::Ask { question } => commands::ai::run_ask(client, _paths, &question).await,
            AiAction::Suggest => commands::ai::run_suggest(client, _paths).await,
            // Setup / Config / Explain are handled before the daemon is started.
            _ => unreachable!(),
        },

        Commands::Monit => commands::monit::run(client).await,
        Commands::Deploy { env } => commands::deploy::run(client, &env).await,
        Commands::Rollback { env } => commands::rollback::run(client, &env).await,
        Commands::Proxy => commands::proxy_cmd::run(client).await,

        // These are handled earlier; this arm is unreachable.
        Commands::Startup
        | Commands::Unstartup
        | Commands::SelfUpdate
        | Commands::Completion { .. }
        | Commands::History { .. }
        | Commands::Logs { .. }
        | Commands::Notify { .. }
        | Commands::Cloud { .. }
        | Commands::Bot { .. } => unreachable!(),
    }
}

// ---------------------------------------------------------------------------
// Cloud command dispatcher
// ---------------------------------------------------------------------------

async fn dispatch_cloud(action: CloudAction, paths: &MhostPaths) -> Result<(), String> {
    use commands::cloud;

    match action {
        CloudAction::Add {
            name,
            host,
            user,
            key,
            port,
        } => cloud::run_add(paths, &name, &host, user.as_deref(), key.as_deref(), port),
        CloudAction::Remove { name } => cloud::run_remove(paths, &name),
        CloudAction::List => cloud::run_list(paths),
        CloudAction::Status => cloud::run_status(paths).await,
        CloudAction::Deploy { server, config } => cloud::run_deploy(paths, &server, &config).await,
        CloudAction::Exec { server, command } => cloud::run_exec(paths, &server, &command).await,
        CloudAction::Logs { server, app } => cloud::run_logs(paths, &server, &app).await,
        CloudAction::Restart { server, app } => cloud::run_restart(paths, &server, &app).await,
        CloudAction::Scale {
            server,
            app,
            instances,
        } => cloud::run_scale(paths, &server, &app, instances).await,
        CloudAction::Sync { config } => cloud::run_sync(paths, &config).await,
        CloudAction::Ssh { server } => cloud::run_ssh(paths, &server),
        CloudAction::Install { server } => cloud::run_install(paths, &server).await,
        CloudAction::Update { target } => cloud::run_update(paths, &target).await,
        CloudAction::Import {
            provider,
            region,
            tag,
        } => cloud::run_import(paths, &provider, region.as_deref(), tag.as_deref()).await,
        CloudAction::AiSetup { description } => cloud::run_ai_setup(paths, &description).await,
        CloudAction::AiDiagnose { server } => cloud::run_ai_diagnose(paths, &server).await,
        CloudAction::AiMigrate { from, to } => cloud::run_ai_migrate(paths, &from, &to).await,
    }
}
