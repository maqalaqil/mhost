use async_trait::async_trait;

use crate::event::NotifyEvent;

/// Trait for notification channels. All channel implementations must be Send + Sync.
#[async_trait]
pub trait NotifyChannel: Send + Sync {
    /// Send a notification event through this channel.
    async fn send(&self, event: &NotifyEvent) -> Result<(), String>;

    /// Return the unique name identifying this channel instance.
    fn channel_name(&self) -> &str;
}
