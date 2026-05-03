use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ModelRef, ReplMessage, ReplMode, ReplSession};

const HISTORY_TITLE_MAX_CHARS: usize = 72;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationSummary {
    pub session_id: Uuid,
    pub title: String,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
    pub message_count: usize,
    pub selected_model: ModelRef,
    pub mode: ReplMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationRecord {
    pub summary: ConversationSummary,
    pub messages: Vec<ReplMessage>,
}

impl ConversationRecord {
    pub fn from_session(session: &ReplSession, captured_at_unix_ms: u64) -> Option<Self> {
        if session.messages.is_empty() {
            return None;
        }

        let messages = session
            .messages
            .iter()
            .map(redact_message)
            .collect::<Vec<_>>();
        let title = conversation_title(&messages);

        Some(Self {
            summary: ConversationSummary {
                session_id: session.id,
                title,
                created_at_unix_ms: captured_at_unix_ms,
                updated_at_unix_ms: captured_at_unix_ms,
                message_count: messages.len(),
                selected_model: session.selected_model.clone(),
                mode: session.mode,
            },
            messages,
        })
    }

    pub fn refresh_from_session(&mut self, session: &ReplSession, captured_at_unix_ms: u64) {
        let messages = session
            .messages
            .iter()
            .map(redact_message)
            .collect::<Vec<_>>();

        self.summary.title = conversation_title(&messages);
        self.summary.updated_at_unix_ms = captured_at_unix_ms;
        self.summary.message_count = messages.len();
        self.summary.selected_model = session.selected_model.clone();
        self.summary.mode = session.mode;
        self.messages = messages;
    }
}

pub fn redact_conversation_text(text: &str) -> String {
    let mut redacted = text.to_string();
    for key in [
        "api_key",
        "apikey",
        "authorization",
        "bearer",
        "coddy_ephemeral_model_credential",
        "google_api_key",
        "nvidia_api_key",
        "nvidia_nim_api_key",
        "openai_api_key",
        "openrouter_api_key",
        "password",
        "secret",
        "token",
    ] {
        redacted = redact_assignment_like_values(&redacted, key);
        redacted = redact_json_like_values(&redacted, key);
    }

    redacted = redact_bearer_values(&redacted);
    for prefix in ["sk-or-", "sk-", "nvapi-", "AIza", "ya29."] {
        redacted = redact_prefixed_tokens(&redacted, prefix);
    }

    redacted
}

fn redact_message(message: &ReplMessage) -> ReplMessage {
    ReplMessage {
        id: message.id,
        role: message.role.clone(),
        text: redact_conversation_text(&message.text),
    }
}

fn conversation_title(messages: &[ReplMessage]) -> String {
    let source = messages
        .iter()
        .find(|message| message.role == "user")
        .or_else(|| messages.first())
        .map(|message| message.text.as_str())
        .unwrap_or("New conversation");
    let normalized = source.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_chars(
        if normalized.is_empty() {
            "New conversation".to_string()
        } else {
            normalized
        },
        HISTORY_TITLE_MAX_CHARS,
    )
}

fn truncate_chars(value: String, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value;
    }

    let mut truncated = value
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}

fn redact_assignment_like_values(text: &str, key: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    let lower = text.to_ascii_lowercase();

    while let Some(relative) = lower[cursor..].find(key) {
        let key_start = cursor + relative;
        output.push_str(&text[cursor..key_start]);
        let key_end = key_start + key.len();
        output.push_str(&text[key_start..key_end]);

        let Some((separator_end, quote)) = assignment_separator(text, key_end) else {
            cursor = key_end;
            continue;
        };

        output.push_str(&text[key_end..separator_end]);
        if text[separator_end..].starts_with("[redacted]") {
            output.push_str("[redacted]");
            cursor = separator_end + "[redacted]".len();
            continue;
        }
        let (value_end, closing_quote) = secret_value_end(text, separator_end, quote);
        output.push_str("[redacted]");
        if let Some(quote) = closing_quote {
            output.push(quote);
        }
        cursor = value_end;
    }

    output.push_str(&text[cursor..]);
    output
}

fn redact_json_like_values(text: &str, key: &str) -> String {
    let quoted_key = format!("\"{key}\"");
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    let lower = text.to_ascii_lowercase();

    while let Some(relative) = lower[cursor..].find(&quoted_key) {
        let key_start = cursor + relative;
        output.push_str(&text[cursor..key_start + quoted_key.len()]);
        let after_key = key_start + quoted_key.len();
        let Some((separator_end, quote)) = json_separator(text, after_key) else {
            cursor = after_key;
            continue;
        };

        output.push_str(&text[after_key..separator_end]);
        if text[separator_end..].starts_with("[redacted]") {
            output.push_str("[redacted]");
            cursor = separator_end + "[redacted]".len();
            continue;
        }
        let (value_end, closing_quote) = secret_value_end(text, separator_end, quote);
        output.push_str("[redacted]");
        if let Some(quote) = closing_quote {
            output.push(quote);
        }
        cursor = value_end;
    }

    output.push_str(&text[cursor..]);
    output
}

fn assignment_separator(text: &str, mut cursor: usize) -> Option<(usize, Option<char>)> {
    cursor = skip_ascii_whitespace(text, cursor);
    if text[cursor..].starts_with('=') || text[cursor..].starts_with(':') {
        cursor += 1;
    } else {
        return None;
    }
    cursor = skip_ascii_whitespace(text, cursor);
    read_optional_quote(text, cursor)
}

fn json_separator(text: &str, mut cursor: usize) -> Option<(usize, Option<char>)> {
    cursor = skip_ascii_whitespace(text, cursor);
    if !text[cursor..].starts_with(':') {
        return None;
    }
    cursor += 1;
    cursor = skip_ascii_whitespace(text, cursor);
    read_optional_quote(text, cursor)
}

fn read_optional_quote(text: &str, cursor: usize) -> Option<(usize, Option<char>)> {
    let quote = text[cursor..]
        .chars()
        .next()
        .filter(|ch| *ch == '"' || *ch == '\'');
    match quote {
        Some(quote) => Some((cursor + quote.len_utf8(), Some(quote))),
        None if cursor < text.len() => Some((cursor, None)),
        None => None,
    }
}

fn skip_ascii_whitespace(text: &str, mut cursor: usize) -> usize {
    while cursor < text.len() && text.as_bytes()[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    cursor
}

fn secret_value_end(text: &str, cursor: usize, quote: Option<char>) -> (usize, Option<char>) {
    if let Some(quote) = quote {
        if let Some(relative) = text[cursor..].find(quote) {
            let closing = cursor + relative;
            return (closing + quote.len_utf8(), Some(quote));
        }
        return (text.len(), None);
    }

    let mut end = cursor;
    while end < text.len() {
        let byte = text.as_bytes()[end];
        if byte.is_ascii_whitespace() || matches!(byte, b',' | b';' | b')' | b'}' | b']') {
            break;
        }
        end += 1;
    }
    (end, None)
}

fn redact_bearer_values(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    let lower = text.to_ascii_lowercase();

    while let Some(relative) = lower[cursor..].find("bearer ") {
        let start = cursor + relative;
        let value_start = start + "bearer ".len();
        output.push_str(&text[cursor..value_start]);
        let (value_end, _) = secret_value_end(text, value_start, None);
        output.push_str("[redacted]");
        cursor = value_end;
    }

    output.push_str(&text[cursor..]);
    output
}

fn redact_prefixed_tokens(text: &str, prefix: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;

    while let Some(relative) = text[cursor..].find(prefix) {
        let start = cursor + relative;
        output.push_str(&text[cursor..start]);
        let mut end = start;
        while end < text.len() {
            let byte = text.as_bytes()[end];
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') {
                end += 1;
            } else {
                break;
            }
        }
        output.push_str("[redacted]");
        cursor = end;
    }

    output.push_str(&text[cursor..]);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_with_messages(messages: Vec<ReplMessage>) -> ReplSession {
        let mut session = ReplSession::new(
            ReplMode::DesktopApp,
            ModelRef {
                provider: "openrouter".to_string(),
                name: "deepseek/deepseek-v4-flash".to_string(),
            },
        );
        session.messages = messages;
        session
    }

    #[test]
    fn conversation_record_redacts_common_provider_secrets() {
        let secret = "sk-or-secret-openrouter-token";
        let nvidia_secret = "nvapi-secret-nvidia-token";
        let session = session_with_messages(vec![ReplMessage {
            id: Uuid::new_v4(),
            role: "user".to_string(),
            text: format!(
                "Use OPENROUTER_API_KEY={secret}, NVIDIA_API_KEY={nvidia_secret} and Authorization: Bearer ya29.google-token"
            ),
        }]);

        let record = ConversationRecord::from_session(&session, 1_777_000_000_000)
            .expect("record should be created");

        let serialized = serde_json::to_string(&record).expect("serialize record");
        assert!(!serialized.contains(secret));
        assert!(!serialized.contains(nvidia_secret));
        assert!(!serialized.contains("ya29.google-token"));
        assert!(serialized.contains("[redacted]"));
    }

    #[test]
    fn conversation_record_uses_redacted_user_title() {
        let session = session_with_messages(vec![ReplMessage {
            id: Uuid::new_v4(),
            role: "user".to_string(),
            text: "Analyze this code with apiKey: \"sk-secret\" please".to_string(),
        }]);

        let record = ConversationRecord::from_session(&session, 42).expect("record");

        assert_eq!(
            record.summary.title,
            "Analyze this code with apiKey: \"[redacted]\" please"
        );
        assert_eq!(record.summary.message_count, 1);
        assert_eq!(record.summary.updated_at_unix_ms, 42);
    }

    #[test]
    fn conversation_record_skips_empty_sessions() {
        let session = session_with_messages(Vec::new());

        assert!(ConversationRecord::from_session(&session, 42).is_none());
    }

    #[test]
    fn redaction_is_idempotent_for_overlapping_key_names() {
        let once = redact_conversation_text("OPENROUTER_API_KEY=sk-or-secret-token");
        let twice = redact_conversation_text(&once);

        assert_eq!(once, "OPENROUTER_API_KEY=[redacted]");
        assert_eq!(twice, once);
    }

    #[test]
    fn refresh_preserves_creation_time_and_updates_messages() {
        let mut session = session_with_messages(vec![ReplMessage {
            id: Uuid::new_v4(),
            role: "user".to_string(),
            text: "first".to_string(),
        }]);
        let mut record = ConversationRecord::from_session(&session, 100).expect("record");
        session.messages.push(ReplMessage {
            id: Uuid::new_v4(),
            role: "assistant".to_string(),
            text: "{\"token\":\"sk-new-secret\"}".to_string(),
        });

        record.refresh_from_session(&session, 200);

        assert_eq!(record.summary.created_at_unix_ms, 100);
        assert_eq!(record.summary.updated_at_unix_ms, 200);
        assert_eq!(record.summary.message_count, 2);
        assert!(!serde_json::to_string(&record)
            .unwrap()
            .contains("sk-new-secret"));
    }
}
