use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::Path,
};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use serde_json::Value;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::{
    redact_conversation_text, ReplEvent, ReplEventEnvelope, ReplEventLog, ReplSession,
    ReplSessionSnapshot,
};

#[derive(Debug)]
pub struct ReplEventBroker {
    log: ReplEventLog,
    sender: broadcast::Sender<ReplEventEnvelope>,
    audit_log: Option<ReplEventAuditLog>,
    audit_error: Option<String>,
}

#[derive(Debug)]
pub struct ReplEventSubscription {
    replay: std::vec::IntoIter<ReplEventEnvelope>,
    receiver: broadcast::Receiver<ReplEventEnvelope>,
}

#[derive(Debug)]
struct ReplEventAuditLog {
    file: File,
}

impl ReplEventBroker {
    pub fn new(session_id: Uuid, capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(1));
        Self {
            log: ReplEventLog::new(session_id),
            sender,
            audit_log: None,
            audit_error: None,
        }
    }

    pub fn new_with_audit_path(
        session_id: Uuid,
        capacity: usize,
        path: impl AsRef<Path>,
    ) -> io::Result<Self> {
        let mut broker = Self::new(session_id, capacity);
        broker.enable_audit_path(path)?;
        Ok(broker)
    }

    pub fn enable_audit_path(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        let mut audit_log = ReplEventAuditLog::open(path)?;
        for envelope in self.log.events_after(0) {
            audit_log.append(&envelope)?;
        }
        self.audit_log = Some(audit_log);
        self.audit_error = None;
        Ok(())
    }

    pub fn publish(
        &mut self,
        event: ReplEvent,
        run_id: Option<Uuid>,
        captured_at_unix_ms: u64,
    ) -> ReplEventEnvelope {
        let envelope = self.log.append(event, run_id, captured_at_unix_ms);
        self.append_audit_event(&envelope);
        let _ = self.sender.send(envelope.clone());
        envelope
    }

    pub fn reset_session(
        &mut self,
        session_id: Uuid,
        captured_at_unix_ms: u64,
    ) -> ReplEventEnvelope {
        self.log.reset_session(session_id);
        self.publish(
            ReplEvent::SessionStarted { session_id },
            None,
            captured_at_unix_ms,
        )
    }

    pub fn subscribe_after(&self, sequence: u64) -> ReplEventSubscription {
        ReplEventSubscription {
            replay: self.log.events_after(sequence).into_iter(),
            receiver: self.sender.subscribe(),
        }
    }

    pub fn events_after(&self, sequence: u64) -> Vec<ReplEventEnvelope> {
        self.log.events_after(sequence)
    }

    pub fn last_sequence(&self) -> u64 {
        self.log.last_sequence()
    }

    pub fn replay(&self, session: ReplSession) -> ReplSession {
        self.log.replay(session)
    }

    pub fn snapshot(&self, session: ReplSession) -> ReplSessionSnapshot {
        self.log.snapshot(session)
    }

    pub fn log(&self) -> &ReplEventLog {
        &self.log
    }

    pub fn last_audit_error(&self) -> Option<&str> {
        self.audit_error.as_deref()
    }

    fn append_audit_event(&mut self, envelope: &ReplEventEnvelope) {
        if let Some(audit_log) = &mut self.audit_log {
            if let Err(error) = audit_log.append(envelope) {
                self.audit_error = Some(error.to_string());
            }
        }
    }
}

impl ReplEventSubscription {
    pub async fn next(&mut self) -> Option<ReplEventEnvelope> {
        if let Some(event) = self.replay.next() {
            return Some(event);
        }

        loop {
            match self.receiver.recv().await {
                Ok(event) => return Some(event),
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}

impl ReplEventAuditLog {
    fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            options.mode(0o600);
        }

        Ok(Self {
            file: options.open(path)?,
        })
    }

    fn append(&mut self, envelope: &ReplEventEnvelope) -> io::Result<()> {
        let mut value = serde_json::to_value(envelope).map_err(io::Error::other)?;
        redact_json_strings(&mut value);
        serde_json::to_writer(&mut self.file, &value).map_err(io::Error::other)?;
        self.file.write_all(b"\n")?;
        self.file.flush()
    }
}

fn redact_json_strings(value: &mut Value) {
    match value {
        Value::String(text) => {
            *text = redact_conversation_text(text);
        }
        Value::Array(items) => {
            for item in items {
                redact_json_strings(item);
            }
        }
        Value::Object(fields) => {
            for value in fields.values_mut() {
                redact_json_strings(value);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ReplMessage, ToolStatus};

    #[tokio::test]
    async fn subscription_replays_history_before_live_events() {
        let session_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let mut broker = ReplEventBroker::new(session_id, 16);

        broker.publish(ReplEvent::VoiceListeningStarted, None, 10);
        broker.publish(ReplEvent::RunStarted { run_id }, Some(run_id), 20);

        let mut subscription = broker.subscribe_after(0);
        let first = subscription.next().await.expect("first replay event");
        let second = subscription.next().await.expect("second replay event");

        assert_eq!(first.sequence, 1);
        assert_eq!(second.sequence, 2);

        broker.publish(ReplEvent::RunCompleted { run_id }, Some(run_id), 30);
        let third = subscription.next().await.expect("live event");

        assert_eq!(third.sequence, 3);
        assert!(matches!(third.event, ReplEvent::RunCompleted { .. }));
    }

    #[test]
    fn broker_exposes_incremental_history_and_last_sequence() {
        let session_id = Uuid::new_v4();
        let mut broker = ReplEventBroker::new(session_id, 16);

        broker.publish(ReplEvent::VoiceListeningStarted, None, 10);
        broker.publish(
            ReplEvent::VoiceTranscriptFinal {
                text: "terminal".to_string(),
            },
            None,
            20,
        );

        let events = broker.events_after(1);

        assert_eq!(broker.last_sequence(), 2);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].sequence, 2);
    }

    #[tokio::test]
    async fn reset_session_keeps_subscribers_and_keeps_monotonic_sequences() {
        let session_id = Uuid::new_v4();
        let mut broker = ReplEventBroker::new(session_id, 16);
        broker.publish(ReplEvent::VoiceListeningStarted, None, 10);
        let mut subscription = broker.subscribe_after(broker.last_sequence());
        let new_session_id = Uuid::new_v4();

        let reset_event = broker.reset_session(new_session_id, 20);
        let live_event = subscription.next().await.expect("live reset event");

        assert_eq!(reset_event.sequence, 2);
        assert_eq!(reset_event.session_id, new_session_id);
        assert_eq!(broker.last_sequence(), 2);
        assert_eq!(live_event, reset_event);
    }

    #[tokio::test]
    async fn subscription_without_replay_waits_for_live_events() {
        let session_id = Uuid::new_v4();
        let mut broker = ReplEventBroker::new(session_id, 16);
        let mut subscription = broker.subscribe_after(broker.last_sequence());

        broker.publish(ReplEvent::VoiceListeningStarted, None, 10);

        let event =
            tokio::time::timeout(std::time::Duration::from_millis(100), subscription.next())
                .await
                .expect("live event before timeout")
                .expect("open subscription");

        assert_eq!(event.sequence, 1);
        assert!(matches!(event.event, ReplEvent::VoiceListeningStarted));
    }

    #[test]
    fn audit_log_persists_redacted_jsonl_events() {
        let session_id = Uuid::new_v4();
        let path = std::env::temp_dir().join(format!("coddy-audit-{session_id}.jsonl"));
        let mut broker =
            ReplEventBroker::new_with_audit_path(session_id, 16, &path).expect("audit broker");

        broker.publish(
            ReplEvent::MessageAppended {
                message: ReplMessage {
                    id: Uuid::new_v4(),
                    role: "user".to_string(),
                    text: "Use OPENAI_API_KEY=sk-live-secret".to_string(),
                },
            },
            None,
            10,
        );
        broker.publish(
            ReplEvent::ToolCompleted {
                name: "shell.run".to_string(),
                status: ToolStatus::Succeeded,
            },
            None,
            11,
        );

        let raw = fs::read_to_string(&path).expect("audit log");
        let _ = fs::remove_file(&path);

        assert_eq!(raw.lines().count(), 2);
        assert!(!raw.contains("sk-live-secret"));
        assert!(raw.contains("[redacted]"));
        assert!(raw.contains("\"sequence\":1"));
        assert!(raw.contains("\"ToolCompleted\""));
        assert!(broker.last_audit_error().is_none());
    }
}
