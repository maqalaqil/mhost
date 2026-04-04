use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::broadcast;

/// An event emitted by the process manager, broadcast to WebSocket
/// clients and webhook dispatchers.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessEvent {
    pub event: String,
    pub process: String,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
}

impl ProcessEvent {
    pub fn new(event: impl Into<String>, process: impl Into<String>) -> Self {
        Self {
            event: event.into(),
            process: process.into(),
            timestamp: Utc::now(),
            detail: None,
        }
    }

    pub fn with_detail(self, detail: serde_json::Value) -> Self {
        Self {
            detail: Some(detail),
            ..self
        }
    }
}

/// Broadcast-based event bus for distributing `ProcessEvent`s to
/// multiple subscribers (WebSocket connections, webhook dispatcher).
#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<ProcessEvent>,
}

impl EventBus {
    /// Creates a new EventBus with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Publishes an event to all current subscribers.
    /// Returns the number of receivers that received the event,
    /// or 0 if there are no active subscribers.
    pub fn publish(&self, event: ProcessEvent) -> usize {
        self.sender.send(event).unwrap_or(0)
    }

    /// Creates a new receiver for subscribing to events.
    pub fn subscribe(&self) -> broadcast::Receiver<ProcessEvent> {
        self.sender.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_event_new() {
        let event = ProcessEvent::new("start", "web-server");
        assert_eq!(event.event, "start");
        assert_eq!(event.process, "web-server");
        assert!(event.detail.is_none());
    }

    #[test]
    fn test_process_event_with_detail() {
        let event =
            ProcessEvent::new("exit", "worker").with_detail(serde_json::json!({ "code": 1 }));
        assert_eq!(event.event, "exit");
        assert_eq!(event.process, "worker");
        let detail = event.detail.unwrap();
        assert_eq!(detail["code"], 1);
    }

    #[test]
    fn test_process_event_immutability() {
        let event = ProcessEvent::new("restart", "api");
        let with_detail = event.clone().with_detail(serde_json::json!("info"));
        // original is unchanged
        assert!(event.detail.is_none());
        assert!(with_detail.detail.is_some());
    }

    #[test]
    fn test_process_event_serialization() {
        let event = ProcessEvent::new("stop", "db");
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["event"], "stop");
        assert_eq!(json["process"], "db");
        assert!(json.get("timestamp").is_some());
        // detail should be absent (skip_serializing_if)
        assert!(json.get("detail").is_none());
    }

    #[test]
    fn test_process_event_serialization_with_detail() {
        let event =
            ProcessEvent::new("error", "worker").with_detail(serde_json::json!({ "msg": "oom" }));
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["detail"]["msg"], "oom");
    }

    #[tokio::test]
    async fn test_event_bus_publish_subscribe() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        let event = ProcessEvent::new("start", "web");
        let receivers = bus.publish(event);
        assert_eq!(receivers, 1);

        let received = rx.recv().await.unwrap();
        assert_eq!(received.event, "start");
        assert_eq!(received.process, "web");
    }

    #[tokio::test]
    async fn test_event_bus_multiple_subscribers() {
        let bus = EventBus::new(16);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        let event = ProcessEvent::new("stop", "api");
        let receivers = bus.publish(event);
        assert_eq!(receivers, 2);

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();
        assert_eq!(e1.event, "stop");
        assert_eq!(e2.event, "stop");
    }

    #[test]
    fn test_event_bus_no_subscribers() {
        let bus = EventBus::new(16);
        let event = ProcessEvent::new("start", "web");
        let receivers = bus.publish(event);
        assert_eq!(receivers, 0);
    }

    #[test]
    fn test_event_bus_clone() {
        let bus = EventBus::new(16);
        let bus2 = bus.clone();
        let mut rx = bus.subscribe();

        bus2.publish(ProcessEvent::new("clone-test", "proc"));
        let received = rx.try_recv().unwrap();
        assert_eq!(received.event, "clone-test");
    }
}
