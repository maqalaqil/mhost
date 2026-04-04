use std::collections::HashSet;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures::stream::SplitSink;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::auth::AuthenticatedUser;
use crate::envelope::ApiError;
use crate::event_bus::ProcessEvent;
use crate::roles::Role;
use crate::server::AppState;

/// Query parameters for the WebSocket upgrade request.
/// Authentication is done via the `token` query parameter since
/// browsers cannot set custom headers on WebSocket connections.
#[derive(Deserialize)]
pub struct WsQuery {
    token: String,
}

/// Handles the WebSocket upgrade request.
///
/// Authenticates the client using the `token` query parameter,
/// then upgrades the connection and spawns the message loop.
pub async fn ws_handler(
    State(state): State<AppState>,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, ApiError> {
    // Authenticate via query token
    let user = {
        let store = state.auth.token_store.lock().await;
        let api_token = store
            .verify(&query.token)
            .ok_or_else(|| ApiError::unauthorized("invalid or expired token"))?
            .clone();

        AuthenticatedUser {
            token_id: api_token.id,
            name: api_token.name,
            role: api_token.role,
        }
    };

    let event_bus = state.event_bus.clone();
    let supervisor = state.supervisor.clone();

    Ok(ws.on_upgrade(move |socket| handle_ws(socket, user, event_bus, supervisor)))
}

/// Incoming messages from the WebSocket client.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WsIncoming {
    Subscribe {
        channel: String,
        process: Option<String>,
    },
    Unsubscribe {
        channel: String,
        process: Option<String>,
    },
    Command {
        action: String,
        process: String,
    },
}

/// Outgoing messages sent to the WebSocket client.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WsOutgoing {
    Event {
        channel: String,
        event: String,
        process: String,
        timestamp: String,
        data: Option<serde_json::Value>,
    },
    Error {
        message: String,
    },
    Ack {
        message: String,
    },
}

/// Tracks which channels and processes the client has subscribed to.
#[derive(Debug, Default)]
struct Subscriptions {
    /// Subscribed channel names: "all", "events", "logs", "metrics"
    channels: HashSet<String>,
    /// If non-empty, only forward events for these process names.
    /// If empty and a channel is subscribed, forward all processes.
    processes: HashSet<String>,
}

impl Subscriptions {
    fn matches(&self, event: &ProcessEvent) -> bool {
        if self.channels.is_empty() {
            return false;
        }

        // Check channel match
        let channel_match = self.channels.contains("all")
            || self.channels.contains("events")
            || (self.channels.contains("logs") && event.event.contains("log"))
            || (self.channels.contains("metrics") && event.event.contains("metric"));

        if !channel_match {
            return false;
        }

        // Check process filter
        if self.processes.is_empty() {
            return true;
        }

        self.processes.contains(&event.process)
    }

    fn subscribe(&mut self, channel: &str, process: Option<&str>) {
        self.channels.insert(channel.to_string());
        if let Some(p) = process {
            self.processes.insert(p.to_string());
        }
    }

    fn unsubscribe(&mut self, channel: &str, process: Option<&str>) {
        if let Some(p) = process {
            self.processes.remove(p);
        } else {
            self.channels.remove(channel);
        }
    }
}

/// Main WebSocket message loop.
///
/// Runs a select loop between incoming client messages and outgoing
/// event bus events, forwarding matching events as JSON to the client.
async fn handle_ws(
    socket: WebSocket,
    user: AuthenticatedUser,
    event_bus: crate::event_bus::EventBus,
    supervisor: std::sync::Arc<dyn crate::server::SupervisorApi>,
) {
    let (mut sender, mut receiver) = socket.split();
    let mut event_rx = event_bus.subscribe();
    let mut subs = Subscriptions::default();

    loop {
        tokio::select! {
            // Incoming message from client
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        handle_client_message(
                            &text,
                            &user,
                            &mut subs,
                            &mut sender,
                            &supervisor,
                        ).await;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::debug!(user = %user.name, "WebSocket client disconnected");
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::warn!(user = %user.name, error = %e, "WebSocket receive error");
                        break;
                    }
                    _ => {
                        // Ignore binary/ping/pong frames
                    }
                }
            }

            // Outgoing event from event bus
            event = event_rx.recv() => {
                match event {
                    Ok(ev) => {
                        if subs.matches(&ev) {
                            let outgoing = WsOutgoing::Event {
                                channel: "events".to_string(),
                                event: ev.event.clone(),
                                process: ev.process.clone(),
                                timestamp: ev.timestamp.to_rfc3339(),
                                data: ev.detail,
                            };
                            if send_json(&mut sender, &outgoing).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            user = %user.name,
                            skipped = n,
                            "WebSocket event receiver lagged"
                        );
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!(user = %user.name, "Event bus closed");
                        break;
                    }
                }
            }
        }
    }
}

/// Processes a single incoming text message from the client.
async fn handle_client_message(
    text: &str,
    user: &AuthenticatedUser,
    subs: &mut Subscriptions,
    sender: &mut SplitSink<WebSocket, Message>,
    supervisor: &std::sync::Arc<dyn crate::server::SupervisorApi>,
) {
    let incoming: WsIncoming = match serde_json::from_str(text) {
        Ok(msg) => msg,
        Err(e) => {
            let _ = send_json(
                sender,
                &WsOutgoing::Error {
                    message: format!("invalid message: {e}"),
                },
            )
            .await;
            return;
        }
    };

    match incoming {
        WsIncoming::Subscribe { channel, process } => {
            subs.subscribe(&channel, process.as_deref());
            let _ = send_json(
                sender,
                &WsOutgoing::Ack {
                    message: format!("subscribed to {channel}"),
                },
            )
            .await;
        }
        WsIncoming::Unsubscribe { channel, process } => {
            subs.unsubscribe(&channel, process.as_deref());
            let _ = send_json(
                sender,
                &WsOutgoing::Ack {
                    message: format!("unsubscribed from {channel}"),
                },
            )
            .await;
        }
        WsIncoming::Command { action, process } => {
            // Commands require Operator role or higher
            if !user.role.has_permission(Role::Operator) {
                let _ = send_json(
                    sender,
                    &WsOutgoing::Error {
                        message: "insufficient permissions for commands".to_string(),
                    },
                )
                .await;
                return;
            }

            let result = match action.as_str() {
                "restart" => supervisor.restart_process(&process).await,
                "stop" => supervisor.stop_process(&process).await,
                _ => {
                    let _ = send_json(
                        sender,
                        &WsOutgoing::Error {
                            message: format!("unknown command: {action}"),
                        },
                    )
                    .await;
                    return;
                }
            };

            match result {
                Ok(()) => {
                    let _ = send_json(
                        sender,
                        &WsOutgoing::Ack {
                            message: format!("{action} {process}: ok"),
                        },
                    )
                    .await;
                }
                Err(e) => {
                    let _ = send_json(
                        sender,
                        &WsOutgoing::Error {
                            message: format!("{action} {process} failed: {e}"),
                        },
                    )
                    .await;
                }
            }
        }
    }
}

/// Sends a JSON-serialized message to the WebSocket client.
async fn send_json(sender: &mut SplitSink<WebSocket, Message>, msg: &WsOutgoing) -> Result<(), ()> {
    let json = serde_json::to_string(msg).map_err(|_| ())?;
    sender.send(Message::Text(json)).await.map_err(|_| ())
}
