use std::collections::HashMap;
use std::time::Duration;

use tracing::{debug, info, warn};

use crate::channel::NotifyChannel;
use crate::event::{EventType, NotifyEvent};
use crate::throttle::Throttle;

/// Central dispatcher that routes events to registered channels,
/// applying throttle and event-type filters.
pub struct NotifyDispatcher {
    channels: HashMap<String, Box<dyn NotifyChannel>>,
    throttle: Throttle,
    /// Maps channel name -> allowed EventTypes. Empty = allow all.
    event_filters: HashMap<String, Vec<EventType>>,
}

impl NotifyDispatcher {
    pub fn new(default_throttle_window: Duration) -> Self {
        Self {
            channels: HashMap::new(),
            throttle: Throttle::new(default_throttle_window),
            event_filters: HashMap::new(),
        }
    }

    /// Register a notification channel.
    pub fn add_channel(&mut self, channel: Box<dyn NotifyChannel>) {
        let name = channel.channel_name().to_string();
        info!(channel = %name, "Registering notification channel");
        self.channels.insert(name, channel);
    }

    /// Set allowed event types for a channel. If not set, all events are allowed.
    pub fn set_event_filter(&mut self, channel_name: &str, allowed: Vec<EventType>) {
        self.event_filters.insert(channel_name.to_string(), allowed);
    }

    /// Returns `true` if the event type is allowed for this channel.
    fn is_allowed(&self, channel_name: &str, event: &NotifyEvent) -> bool {
        match self.event_filters.get(channel_name) {
            None => true,
            Some(allowed) if allowed.is_empty() => true,
            Some(allowed) => allowed.contains(&event.event_type),
        }
    }

    /// Dispatch the event to all registered channels that pass filters and throttle.
    pub async fn dispatch(&mut self, event: &NotifyEvent) {
        let channel_names: Vec<String> = self.channels.keys().cloned().collect();

        for name in channel_names {
            if !self.is_allowed(&name, event) {
                debug!(channel = %name, event_type = %event.event_type, "Event filtered out");
                continue;
            }

            if !self.throttle.should_send_default(&name) {
                warn!(
                    channel = %name,
                    process = %event.process_name,
                    "Notification throttled"
                );
                continue;
            }

            if let Some(channel) = self.channels.get(&name) {
                match channel.send(event).await {
                    Ok(()) => {
                        info!(channel = %name, process = %event.process_name, "Notification sent");
                    }
                    Err(e) => {
                        warn!(channel = %name, error = %e, "Notification send failed");
                    }
                }
            }
        }
    }

    /// Dispatch to a specific channel by name, bypassing throttle.
    pub async fn dispatch_to(&self, channel_name: &str, event: &NotifyEvent) -> Result<(), String> {
        let channel = self
            .channels
            .get(channel_name)
            .ok_or_else(|| format!("Channel '{channel_name}' not found"))?;

        channel.send(event).await
    }

    /// Returns the number of registered channels.
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// Returns whether a channel with the given name is registered.
    pub fn has_channel(&self, name: &str) -> bool {
        self.channels.contains_key(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventType, NotifyEvent, Severity};
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    /// A test channel that records all received events.
    struct RecordingChannel {
        name: String,
        events: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl NotifyChannel for RecordingChannel {
        async fn send(&self, event: &NotifyEvent) -> Result<(), String> {
            self.events.lock().unwrap().push(event.process_name.clone());
            Ok(())
        }
        fn channel_name(&self) -> &str {
            &self.name
        }
    }

    fn make_event(event_type: EventType) -> NotifyEvent {
        NotifyEvent::new(event_type, "test-svc", "test message", Severity::Critical)
    }

    #[tokio::test]
    async fn test_dispatcher_sends_to_all_channels() {
        let events1 = Arc::new(Mutex::new(Vec::new()));
        let events2 = Arc::new(Mutex::new(Vec::new()));

        let mut dispatcher = NotifyDispatcher::new(Duration::from_secs(0));
        dispatcher.add_channel(Box::new(RecordingChannel {
            name: "ch1".to_string(),
            events: Arc::clone(&events1),
        }));
        dispatcher.add_channel(Box::new(RecordingChannel {
            name: "ch2".to_string(),
            events: Arc::clone(&events2),
        }));

        dispatcher.dispatch(&make_event(EventType::Crash)).await;

        assert_eq!(events1.lock().unwrap().len(), 1);
        assert_eq!(events2.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_dispatcher_throttle_suppresses_second_send() {
        let events = Arc::new(Mutex::new(Vec::new()));

        let mut dispatcher = NotifyDispatcher::new(Duration::from_secs(60));
        dispatcher.add_channel(Box::new(RecordingChannel {
            name: "throttled".to_string(),
            events: Arc::clone(&events),
        }));

        let event = make_event(EventType::Crash);
        dispatcher.dispatch(&event).await;
        dispatcher.dispatch(&event).await;

        // Only first dispatch should reach the channel
        assert_eq!(events.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_dispatcher_throttle_allows_after_window_expires() {
        let events = Arc::new(Mutex::new(Vec::new()));

        let mut dispatcher = NotifyDispatcher::new(Duration::from_millis(10));
        dispatcher.add_channel(Box::new(RecordingChannel {
            name: "fast-throttle".to_string(),
            events: Arc::clone(&events),
        }));

        let event = make_event(EventType::Crash);
        dispatcher.dispatch(&event).await;
        std::thread::sleep(Duration::from_millis(15));
        dispatcher.dispatch(&event).await;

        assert_eq!(events.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_event_filter_blocks_disallowed_type() {
        let events = Arc::new(Mutex::new(Vec::new()));

        let mut dispatcher = NotifyDispatcher::new(Duration::from_secs(0));
        dispatcher.add_channel(Box::new(RecordingChannel {
            name: "filtered".to_string(),
            events: Arc::clone(&events),
        }));
        // Only allow Crash events on this channel
        dispatcher.set_event_filter("filtered", vec![EventType::Crash]);

        // Deploy event should be filtered out
        dispatcher.dispatch(&make_event(EventType::Deploy)).await;
        assert_eq!(events.lock().unwrap().len(), 0);

        // Crash event should pass through
        dispatcher.dispatch(&make_event(EventType::Crash)).await;
        assert_eq!(events.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_dispatch_to_bypasses_throttle() {
        let events = Arc::new(Mutex::new(Vec::new()));

        let mut dispatcher = NotifyDispatcher::new(Duration::from_secs(60));
        dispatcher.add_channel(Box::new(RecordingChannel {
            name: "direct".to_string(),
            events: Arc::clone(&events),
        }));

        let event = make_event(EventType::Crash);
        dispatcher.dispatch_to("direct", &event).await.unwrap();
        dispatcher.dispatch_to("direct", &event).await.unwrap();

        assert_eq!(events.lock().unwrap().len(), 2);
    }

    #[test]
    fn test_channel_count() {
        let mut dispatcher = NotifyDispatcher::new(Duration::from_secs(60));
        assert_eq!(dispatcher.channel_count(), 0);

        dispatcher.add_channel(Box::new(RecordingChannel {
            name: "ch".to_string(),
            events: Arc::new(Mutex::new(Vec::new())),
        }));

        assert_eq!(dispatcher.channel_count(), 1);
        assert!(dispatcher.has_channel("ch"));
        assert!(!dispatcher.has_channel("missing"));
    }

    // -- Dispatch with no channels configured — no panic -------------------

    #[tokio::test]
    async fn test_dispatch_with_no_channels() {
        let mut dispatcher = NotifyDispatcher::new(Duration::from_secs(0));
        // Should complete without panic or error
        dispatcher.dispatch(&make_event(EventType::Crash)).await;
        assert_eq!(dispatcher.channel_count(), 0);
    }

    // -- dispatch_to with unknown channel returns error --------------------

    #[tokio::test]
    async fn test_dispatch_to_unknown_channel_returns_error() {
        let dispatcher = NotifyDispatcher::new(Duration::from_secs(0));
        let result = dispatcher
            .dispatch_to("nonexistent", &make_event(EventType::Crash))
            .await;
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("nonexistent"),
            "error should mention the channel name, got: {msg}"
        );
    }

    // -- Event filter with empty allowed list allows everything ------------

    #[tokio::test]
    async fn test_event_filter_empty_allows_all() {
        let events = Arc::new(Mutex::new(Vec::new()));

        let mut dispatcher = NotifyDispatcher::new(Duration::from_secs(0));
        dispatcher.add_channel(Box::new(RecordingChannel {
            name: "all-events".to_string(),
            events: Arc::clone(&events),
        }));
        // Set empty filter — should allow all events
        dispatcher.set_event_filter("all-events", vec![]);

        dispatcher.dispatch(&make_event(EventType::Crash)).await;
        dispatcher.dispatch(&make_event(EventType::Deploy)).await;
        dispatcher.dispatch(&make_event(EventType::Restart)).await;

        assert_eq!(events.lock().unwrap().len(), 3);
    }

    // -- Multiple channels, one with filter --------------------------------

    #[tokio::test]
    async fn test_filter_only_blocks_one_channel() {
        let events_filtered = Arc::new(Mutex::new(Vec::new()));
        let events_open = Arc::new(Mutex::new(Vec::new()));

        let mut dispatcher = NotifyDispatcher::new(Duration::from_secs(0));
        dispatcher.add_channel(Box::new(RecordingChannel {
            name: "crash-only".to_string(),
            events: Arc::clone(&events_filtered),
        }));
        dispatcher.add_channel(Box::new(RecordingChannel {
            name: "all-open".to_string(),
            events: Arc::clone(&events_open),
        }));
        dispatcher.set_event_filter("crash-only", vec![EventType::Crash]);

        // Send a Deploy event
        dispatcher.dispatch(&make_event(EventType::Deploy)).await;

        // crash-only should NOT receive it; all-open should
        assert_eq!(events_filtered.lock().unwrap().len(), 0);
        assert_eq!(events_open.lock().unwrap().len(), 1);
    }
}
