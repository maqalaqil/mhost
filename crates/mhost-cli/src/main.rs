mod cli;
mod commands;
mod daemon_launcher;
pub mod embedded;
mod output;
pub mod resolve;

use clap::Parser;
use mhost_core::paths::MhostPaths;
use mhost_ipc::IpcClient;

use cli::{
    AgentAction, AiAction, BotAction, BrainAction, Cli, CloudAction, Commands, DockerAction,
    HooksAction, LogAlertAction, MetricsAction, NotifyAction, PluginAction, SecretsAction,
    SnapshotAction, StatusPageAction, TemplateAction, WorkspaceAction,
};

// Bring new top-level commands into scope for the dispatch match

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
            follow,
        } => {
            // --follow requires a process name and reads files directly
            if follow {
                match name {
                    Some(n) => commands::logs::follow(paths, &n, err, grep.as_deref()),
                    None => Err("--follow requires a process name".into()),
                }
            } else if search.is_some() || count_by.is_some() {
                // --search and --count-by require daemon access
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
            BotAction::Enable => {
                daemon_launcher::ensure_daemon_running(paths)?;
                let client = IpcClient::new(&paths.socket());
                commands::bot::run_enable(paths, &client).await
            }
            BotAction::RunInline => commands::bot::run_inline(paths).await,
            BotAction::Disable => commands::bot::run_disable(paths).await,
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
            BotAction::ChatId { token } => commands::bot::run_chat_id(&token).await,
        },

        // ---- Agent commands (Setup/Status don't need daemon) -------------
        Commands::Agent { action } => match action {
            AgentAction::Setup => commands::agent::run_setup(paths),
            AgentAction::Status => commands::agent::run_status(paths),
            AgentAction::Start => {
                daemon_launcher::ensure_daemon_running(paths)?;
                let client = IpcClient::new(&paths.socket());
                commands::agent::run_start(paths, &client).await
            }
            AgentAction::Stop => {
                daemon_launcher::ensure_daemon_running(paths)?;
                let client = IpcClient::new(&paths.socket());
                commands::agent::run_stop(&client).await
            }
        },

        // ---- Brain commands (all read JSON files — no daemon needed) -----
        Commands::Brain { action } => match action {
            BrainAction::Status => commands::brain::run_status(paths),
            BrainAction::History => commands::brain::run_history(paths),
            BrainAction::Playbooks => commands::brain::run_playbooks(paths),
            BrainAction::Explain { process } => commands::brain::run_explain(paths, &process),
        },

        // ---- Init (non-daemon, scans CWD for project files) ----------------
        Commands::Init => commands::init::run(),

        // ---- Log alerts (non-daemon, manages ~/.mhost/log-alerts.json) ----
        Commands::LogAlert { action } => match action {
            LogAlertAction::Add {
                process,
                pattern,
                notify,
                cooldown,
            } => commands::log_alerts::run_add(&process, &pattern, &notify, cooldown),
            LogAlertAction::List => commands::log_alerts::run_list(),
            LogAlertAction::Remove { id } => commands::log_alerts::run_remove(&id),
        },

        // ---- API commands (non-daemon, manage tokens/webhooks/server) ------
        Commands::Api { action } => commands::api::run(action).await,

        // ---- Replay (non-daemon, reads brain files) -----------------------
        Commands::Replay { process, time } => {
            commands::replay::run(paths, &process, time.as_deref())
        }

        // ---- Bench (non-daemon, direct HTTP) ------------------------------
        Commands::Bench {
            url,
            duration,
            concurrency,
        } => commands::bench::run(&url, duration, concurrency).await,

        // ---- Link (non-daemon, reads DB) ----------------------------------
        Commands::Link => commands::link::run(paths),

        // ---- Dev mode (non-daemon, runs directly with file watching) ------
        Commands::Dev {
            script,
            watch,
            ext,
            env,
        } => commands::dev::run(&script, watch.as_deref(), ext.as_deref(), env.as_deref()),

        // ---- Dashboard (non-daemon, runs Node.js server) ------------------
        Commands::Dashboard { port } => commands::dashboard::run(port),

        // ---- Certs (non-daemon, uses openssl CLI) -------------------------
        Commands::Certs { url } => commands::certs::run(url).await,

        // ---- Sla (non-daemon, reads brain files) --------------------------
        Commands::Sla { app, target } => commands::sla::run(paths, &app, target),

        // ---- Migrate (non-daemon, reads PM2 dump) -------------------------
        Commands::Migrate { from } => commands::migrate::run(&from),

        // ---- Team (non-daemon, stub) --------------------------------------
        Commands::Team => commands::team::run(),

        // ---- Playground (non-daemon, stub) --------------------------------
        Commands::Playground => commands::playground::run(),

        // ---- Run recipe (non-daemon, shell out) ---------------------------
        Commands::Run { file } => commands::recipe::run(&file),

        // ---- Diff (non-daemon, reads fleet.json) --------------------------
        Commands::Diff { env_a, env_b } => commands::diff::run(paths, &env_a, &env_b),

        // ---- Docker commands (non-daemon, shells out to docker CLI) --------
        Commands::Docker { action } => match action {
            DockerAction::Run {
                image,
                name,
                port,
                envs,
            } => commands::docker::run_docker_run(&image, &name, port, &envs),
            DockerAction::List => commands::docker::run_docker_list(),
            DockerAction::Stop { name } => commands::docker::run_docker_stop(&name),
            DockerAction::Restart { name } => commands::docker::run_docker_restart(&name),
            DockerAction::Logs { name, lines } => commands::docker::run_docker_logs(&name, lines),
            DockerAction::Rm { name } => commands::docker::run_docker_rm(&name),
            DockerAction::Pull { image } => commands::docker::run_docker_pull(&image),
        },

        // ---- Template commands (non-daemon, generates files) -------------
        Commands::Template { action } => match action {
            TemplateAction::List => commands::template::run_list(),
            TemplateAction::Init { name } => commands::template::run_init(&name),
        },

        // ---- Plugin commands (non-daemon, reads ~/.mhost/plugins/) ---------
        Commands::Plugin { action } => match action {
            PluginAction::List => commands::plugin::run_list(),
            PluginAction::Install { path } => commands::plugin::run_install(&path),
            PluginAction::Remove { name } => commands::plugin::run_remove(&name),
            PluginAction::Info { name } => commands::plugin::run_info(&name),
        },

        // ---- Audit trail (non-daemon, reads ~/.mhost/audit.jsonl) --------
        Commands::Audit {
            process,
            since,
            limit,
        } => commands::audit::run(process.as_deref(), since.as_deref(), limit),

        // ---- Watch config (non-daemon, polling loop) ---------------------
        Commands::Watch { config } => commands::watch::run(config.as_deref()),

        // ---- Process rollback (non-daemon, reads DB + dump) --------------
        Commands::RollbackProcess { process } => {
            commands::process_rollback::run_rollback(paths, &process)
        }

        // ---- Config history (non-daemon, reads DB + dump) ----------------
        Commands::ConfigHistory { process } => {
            commands::process_rollback::run_config_history(paths, &process)
        }

        // ---- Snapshot list (non-daemon, reads files) ----------------------
        Commands::Snapshot {
            action: SnapshotAction::List,
        } => commands::snapshot::list(paths),

        // ---- Workspace commands (non-daemon, reads/writes ~/.mhost/) -----
        Commands::Workspace { action } => match action {
            WorkspaceAction::List => commands::workspace::run_list(),
            WorkspaceAction::Create { name } => commands::workspace::run_create(&name),
            WorkspaceAction::Switch { name } => commands::workspace::run_switch(&name),
            WorkspaceAction::Current => commands::workspace::run_current(),
            WorkspaceAction::Delete { name } => commands::workspace::run_delete(&name),
        },

        // ---- Status page (non-daemon, generates HTML) --------------------
        Commands::StatusPage { port, action } => match action {
            Some(StatusPageAction::Generate) => commands::status_page::run_generate(),
            None => commands::status_page::run_serve(port),
        },

        // ---- Incoming hooks (non-daemon, manages ~/.mhost/incoming-hooks.json)
        Commands::Hooks { action } => match action {
            HooksAction::Create { action, process } => {
                commands::incoming_hooks::run_create(&action, &process)
            }
            HooksAction::List => commands::incoming_hooks::run_list(),
            HooksAction::Remove { id } => commands::incoming_hooks::run_remove(&id),
            HooksAction::Test { id } => commands::incoming_hooks::run_test(&id),
        },

        // ---- Reload (needs daemon) ----------------------------------------
        Commands::Reload { target } => {
            daemon_launcher::ensure_daemon_running(paths)?;
            let client = IpcClient::new(&paths.socket());
            let name = resolve::resolve_target(&client, &target).await?;
            commands::reload::run(&client, &name).await
        }

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
            tags,
            cpu_limit,
            memory_limit,
        } => {
            if let Some(ref g) = group {
                commands::group::start(client, g).await
            } else {
                commands::start::run(
                    client,
                    &target,
                    name.as_deref(),
                    &tags,
                    cpu_limit.as_deref(),
                    memory_limit,
                )
                .await
            }
        }
        Commands::Stop { target, group } => {
            if let Some(ref g) = group {
                commands::group::stop(client, g).await
            } else {
                let name = resolve::resolve_target(client, &target).await?;
                commands::stop::run(client, &name).await
            }
        }
        Commands::Restart { target } => {
            let name = resolve::resolve_target(client, &target).await?;
            commands::restart::run(client, &name).await
        }
        Commands::Delete { target } => {
            let name = resolve::resolve_target(client, &target).await?;
            commands::delete::run(client, &name).await
        }
        Commands::List { tag } => commands::list::run(client, tag.as_deref()).await,
        Commands::Info { name } => {
            let resolved = resolve::resolve_target(client, &name).await?;
            commands::info::run(client, &resolved).await
        }
        Commands::Env { name } => {
            let resolved = resolve::resolve_target(client, &name).await?;
            commands::env_cmd::run(client, &resolved).await
        }
        Commands::Scale { name, instances } => {
            let resolved = resolve::resolve_target(client, &name).await?;
            commands::scale::run(client, &resolved, instances).await
        }
        Commands::Save => commands::save::run(client).await,
        Commands::Resurrect => commands::resurrect::run(client).await,
        Commands::Ping => commands::ping::run(client).await,
        Commands::Kill => commands::kill::run(client).await,
        Commands::Config { name } => {
            let resolved = resolve::resolve_target(client, &name).await?;
            commands::config_cmd::run(client, &resolved).await
        }
        Commands::Health { name } => {
            let resolved = resolve::resolve_target(client, &name).await?;
            commands::health::run(client, &resolved).await
        }
        Commands::Cluster { name, instances } => {
            let resolved = resolve::resolve_target(client, &name).await?;
            commands::cluster::run(client, &resolved, instances).await
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

        Commands::Cost => commands::cost::run(client).await,

        Commands::Monit => commands::monit::run(client).await,
        Commands::Deploy { env } => commands::deploy::run(client, &env).await,
        Commands::Rollback { env } => commands::rollback::run(client, &env).await,
        Commands::Proxy => commands::proxy_cmd::run(client).await,

        // ---- Canary (needs daemon) ----------------------------------------
        Commands::Canary {
            app,
            percent,
            duration,
        } => commands::canary::run(client, &app, percent, duration).await,

        // ---- Share (needs daemon to detect port) --------------------------
        Commands::Share { app, port } => commands::share::run(client, &app, port).await,

        // ---- Snapshot create / restore (need daemon) ----------------------
        Commands::Snapshot {
            action: SnapshotAction::Create { name },
        } => commands::snapshot::create(client, _paths, &name).await,
        Commands::Snapshot {
            action: SnapshotAction::Restore { name },
        } => commands::snapshot::restore(client, _paths, &name).await,

        // ---- Cron dashboard (needs daemon to list processes) ----------------
        Commands::Cron => commands::cron::run(client).await,

        // ---- Limits (needs daemon for metrics) ----------------------------
        Commands::Limits { process } => {
            let resolved = resolve::resolve_target(client, &process).await?;
            commands::limits::run(client, &resolved).await
        }

        // These are handled earlier; this arm is unreachable.
        Commands::Startup
        | Commands::Unstartup
        | Commands::SelfUpdate
        | Commands::Completion { .. }
        | Commands::History { .. }
        | Commands::Logs { .. }
        | Commands::Notify { .. }
        | Commands::Cloud { .. }
        | Commands::Bot { .. }
        | Commands::Agent { .. }
        | Commands::Brain { .. }
        | Commands::Dev { .. }
        | Commands::Dashboard { .. }
        | Commands::Reload { .. }
        | Commands::Replay { .. }
        | Commands::Bench { .. }
        | Commands::Link
        | Commands::Certs { .. }
        | Commands::Sla { .. }
        | Commands::Migrate { .. }
        | Commands::Team
        | Commands::Playground
        | Commands::Run { .. }
        | Commands::Diff { .. }
        | Commands::Api { .. }
        | Commands::Init
        | Commands::LogAlert { .. }
        | Commands::Docker { .. }
        | Commands::Template { .. }
        | Commands::Plugin { .. }
        | Commands::Audit { .. }
        | Commands::Watch { .. }
        | Commands::RollbackProcess { .. }
        | Commands::ConfigHistory { .. }
        | Commands::Snapshot {
            action: SnapshotAction::List,
        }
        | Commands::Workspace { .. }
        | Commands::StatusPage { .. }
        | Commands::Hooks { .. } => unreachable!(),
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
        CloudAction::Auth { provider } => cloud::run_auth(paths, &provider),
        CloudAction::AuthList => cloud::run_auth_list(paths),
        CloudAction::AuthRemove { provider } => cloud::run_auth_remove(paths, &provider),
        CloudAction::AiSetup { description } => cloud::run_ai_setup(paths, &description).await,
        CloudAction::AiDiagnose { server } => cloud::run_ai_diagnose(paths, &server).await,
        CloudAction::AiMigrate { from, to } => cloud::run_ai_migrate(paths, &from, &to).await,

        // ── Cloud-Native commands ──────────────────────────────────
        CloudAction::Provision {
            provider,
            name,
            r#type,
            image,
            port,
            instances,
            region,
            cpu,
            memory,
        } => {
            cloud::run_cloud_provision(
                paths,
                &provider,
                &name,
                &r#type,
                image.as_deref(),
                port,
                instances,
                region.as_deref(),
                cpu.as_deref(),
                memory.as_deref(),
            );
            Ok(())
        }
        CloudAction::Services { provider } => {
            cloud::run_cloud_services(paths, provider.as_deref());
            Ok(())
        }
        CloudAction::Service { name, provider } => {
            cloud::run_cloud_service(paths, &name, provider.as_deref());
            Ok(())
        }
        CloudAction::CloudDeploy {
            name,
            image,
            provider,
        } => {
            cloud::run_cloud_deploy_image(paths, &name, &image, provider.as_deref());
            Ok(())
        }
        CloudAction::CloudScale {
            name,
            instances,
            provider,
        } => {
            cloud::run_cloud_scale_native(paths, &name, instances, provider.as_deref());
            Ok(())
        }
        CloudAction::Destroy {
            name,
            provider,
            confirm,
        } => cloud::run_cloud_destroy(paths, &name, &provider, confirm),
        CloudAction::Cost { provider } => {
            cloud::run_cloud_cost(paths, provider.as_deref());
            Ok(())
        }
        CloudAction::Drift { fix } => {
            cloud::run_cloud_drift(paths, fix);
            Ok(())
        }
        CloudAction::Secrets { action } => match action {
            SecretsAction::Set {
                service,
                key,
                value,
            } => {
                cloud::run_cloud_secrets_set(paths, &service, &key, &value);
                Ok(())
            }
            SecretsAction::List { service } => {
                cloud::run_cloud_secrets_list(paths, &service);
                Ok(())
            }
            SecretsAction::Remove { service, key } => {
                cloud::run_cloud_secrets_remove(paths, &service, &key);
                Ok(())
            }
        },
        CloudAction::Backup { service } => {
            cloud::run_cloud_backup(paths, &service);
            Ok(())
        }
        CloudAction::BackupList => {
            cloud::run_cloud_backup_list(paths);
            Ok(())
        }
        CloudAction::Export { format } => {
            cloud::run_cloud_export(&format);
            Ok(())
        }
    }
}
