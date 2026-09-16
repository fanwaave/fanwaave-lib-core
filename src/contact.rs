#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactChannel {
    Email,
    Sms,
}

impl ContactChannel {
    pub const fn provider(self) -> ContactProvider {
        match self {
            Self::Email => ContactProvider::Sendgrid,
            Self::Sms => ContactProvider::Twilio,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::Sms => "sms",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactProvider {
    Sendgrid,
    Twilio,
}

impl ContactProvider {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sendgrid => "sendgrid",
            Self::Twilio => "twilio",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactJobStatus {
    Queued,
    Leased,
    Retry,
    Sent,
    Dead,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailContactPayload {
    pub to: String,
    pub subject: String,
    pub html: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub custom_args: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmsContactPayload {
    pub to: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_callback: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "channel", content = "payload", rename_all = "snake_case")]
pub enum ContactPayload {
    Email(EmailContactPayload),
    Sms(SmsContactPayload),
}

impl ContactPayload {
    pub const fn channel(&self) -> ContactChannel {
        match self {
            Self::Email(_) => ContactChannel::Email,
            Self::Sms(_) => ContactChannel::Sms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnqueueContactJob {
    pub idempotency_key: String,
    pub payload: ContactPayload,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub campaign_id: Option<String>,
    #[serde(default)]
    pub priority: i32,
    #[serde(default = "default_max_attempts")]
    pub max_attempts: i32,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnqueueContactReceipt {
    pub job_id: String,
    pub idempotency_key: String,
    pub status: ContactJobStatus,
    pub duplicate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactJobView {
    pub job_id: String,
    pub idempotency_key: String,
    pub channel: ContactChannel,
    pub provider: ContactProvider,
    pub status: ContactJobStatus,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub provider_message_id: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuppressionReason {
    Unsubscribe,
    Bounce,
    Complaint,
    Invalid,
    Manual,
}

const fn default_max_attempts() -> i32 {
    8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_is_derived_from_channel() {
        assert_eq!(ContactChannel::Email.provider(), ContactProvider::Sendgrid);
        assert_eq!(ContactChannel::Sms.provider(), ContactProvider::Twilio);
    }

    #[test]
    fn payload_channel_is_not_ambiguous() {
        let payload = ContactPayload::Sms(SmsContactPayload {
            to: "+15551234567".into(),
            body: "hello".into(),
            from: None,
            status_callback: None,
        });
        assert_eq!(payload.channel(), ContactChannel::Sms);
    }
}
