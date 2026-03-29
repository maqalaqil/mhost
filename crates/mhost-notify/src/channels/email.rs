use async_trait::async_trait;
use lettre::{
    message::{header::ContentType, Mailbox, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    Address, Message, SmtpTransport, Transport,
};
use tracing::error;

use crate::channel::NotifyChannel;
use crate::event::{NotifyEvent, Severity};

/// SMTP email notification channel.
pub struct EmailChannel {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub from: String,
    pub to: Vec<String>,
    pub username: String,
    pub password: String,
    pub name: String,
}

impl EmailChannel {
    pub fn new(
        name: impl Into<String>,
        smtp_host: impl Into<String>,
        smtp_port: u16,
        from: impl Into<String>,
        to: Vec<String>,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            smtp_host: smtp_host.into(),
            smtp_port,
            from: from.into(),
            to,
            username: username.into(),
            password: password.into(),
        }
    }

    /// Build the plain text version of the email.
    pub fn format_text_body(event: &NotifyEvent) -> String {
        let timestamp = event.timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string();
        format!(
            "mhost Notification\n\
            ==================\n\
            Event:    {}\n\
            Process:  {}\n\
            Severity: {}\n\
            Message:  {}\n\
            Time:     {}\n",
            event.event_type,
            event.process_name,
            event.severity,
            event.message,
            timestamp,
        )
    }

    /// Build the HTML email body.
    pub fn format_html_body(event: &NotifyEvent) -> String {
        let timestamp = event.timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string();

        let severity_color = match event.severity {
            Severity::Critical => "#E01E5A",
            Severity::Warning => "#ECB22E",
            Severity::Info => "#2EB67D",
        };

        format!(
            r#"<!DOCTYPE html>
<html>
<body style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto; padding: 20px;">
  <div style="background-color: {severity_color}; color: white; padding: 15px; border-radius: 5px 5px 0 0;">
    <h2 style="margin: 0;">mhost — {event_type}</h2>
  </div>
  <div style="border: 1px solid #ddd; padding: 20px; border-radius: 0 0 5px 5px;">
    <table style="width: 100%; border-collapse: collapse;">
      <tr>
        <td style="font-weight: bold; padding: 8px; width: 120px;">Process:</td>
        <td style="padding: 8px;">{process_name}</td>
      </tr>
      <tr style="background-color: #f9f9f9;">
        <td style="font-weight: bold; padding: 8px;">Severity:</td>
        <td style="padding: 8px; color: {severity_color}; font-weight: bold;">{severity}</td>
      </tr>
      <tr>
        <td style="font-weight: bold; padding: 8px;">Message:</td>
        <td style="padding: 8px;">{message}</td>
      </tr>
      <tr style="background-color: #f9f9f9;">
        <td style="font-weight: bold; padding: 8px;">Time:</td>
        <td style="padding: 8px;">{timestamp}</td>
      </tr>
    </table>
  </div>
  <p style="color: #999; font-size: 12px; text-align: center;">Sent by mhost process manager</p>
</body>
</html>"#,
            severity_color = severity_color,
            event_type = event.event_type,
            process_name = html_escape(&event.process_name),
            severity = event.severity,
            message = html_escape(&event.message),
            timestamp = timestamp,
        )
    }

    /// Build the email subject line.
    pub fn format_subject(event: &NotifyEvent) -> String {
        format!(
            "[mhost] [{severity}] {event_type} — {process}",
            severity = event.severity,
            event_type = event.event_type,
            process = event.process_name,
        )
    }
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[async_trait]
impl NotifyChannel for EmailChannel {
    async fn send(&self, event: &NotifyEvent) -> Result<(), String> {
        let from_addr: Address = self
            .from
            .parse()
            .map_err(|e| format!("Invalid from address: {e}"))?;
        let from_mailbox = Mailbox::new(Some("mhost".to_string()), from_addr);

        let subject = Self::format_subject(event);
        let text_body = Self::format_text_body(event);
        let html_body = Self::format_html_body(event);

        let mut message_builder = Message::builder()
            .from(from_mailbox)
            .subject(subject);

        for recipient in &self.to {
            let to_addr: Address = recipient
                .parse()
                .map_err(|e| format!("Invalid to address '{recipient}': {e}"))?;
            message_builder = message_builder.to(Mailbox::new(None, to_addr));
        }

        let email = message_builder
            .multipart(
                MultiPart::alternative()
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_PLAIN)
                            .body(text_body),
                    )
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_HTML)
                            .body(html_body),
                    ),
            )
            .map_err(|e| format!("Failed to build email: {e}"))?;

        let creds = Credentials::new(self.username.clone(), self.password.clone());

        let mailer = SmtpTransport::relay(&self.smtp_host)
            .map_err(|e| format!("SMTP relay error: {e}"))?
            .credentials(creds)
            .port(self.smtp_port)
            .build();

        mailer.send(&email).map_err(|e| {
            error!(channel = %self.name, error = %e, "Email send failed");
            format!("Failed to send email: {e}")
        })?;

        Ok(())
    }

    fn channel_name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventType, NotifyEvent};

    fn make_event(severity: Severity) -> NotifyEvent {
        NotifyEvent::new(EventType::Crash, "payment-service", "OOM killed", severity)
    }

    #[test]
    fn test_format_subject_contains_severity_and_process() {
        let event = make_event(Severity::Critical);
        let subject = EmailChannel::format_subject(&event);
        assert!(subject.contains("CRITICAL"));
        assert!(subject.contains("payment-service"));
        assert!(subject.contains("Crash"));
    }

    #[test]
    fn test_format_text_body_contains_all_fields() {
        let event = make_event(Severity::Warning);
        let body = EmailChannel::format_text_body(&event);
        assert!(body.contains("Crash"));
        assert!(body.contains("payment-service"));
        assert!(body.contains("WARNING"));
        assert!(body.contains("OOM killed"));
        assert!(body.contains("UTC"));
    }

    #[test]
    fn test_format_html_body_contains_severity_color_critical() {
        let event = make_event(Severity::Critical);
        let html = EmailChannel::format_html_body(&event);
        assert!(html.contains("#E01E5A"));
        assert!(html.contains("payment-service"));
        assert!(html.contains("OOM killed"));
    }

    #[test]
    fn test_format_html_body_contains_severity_color_info() {
        let event = make_event(Severity::Info);
        let html = EmailChannel::format_html_body(&event);
        assert!(html.contains("#2EB67D"));
    }

    #[test]
    fn test_html_escape_prevents_xss() {
        let event = NotifyEvent::new(
            EventType::Crash,
            "<script>alert(1)</script>",
            "msg",
            Severity::Info,
        );
        let html = EmailChannel::format_html_body(&event);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }
}
