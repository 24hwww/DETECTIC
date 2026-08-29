use std::path::Path;

use rusqlite::params;
use rusqlite::OptionalExtension;

use crate::notifier::{DetectionEvent, SmtpError};

#[derive(Debug, Clone, PartialEq)]
pub struct Email {
    pub to: String,
    pub from: String,
    pub subject: String,
    pub body_text: String,
    pub body_html: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingEmail {
    pub id: i64,
    pub created_at: i64,
    pub retry_count: u32,
    pub next_attempt: i64,
    pub to: String,
    pub from: String,
    pub subject: String,
    pub body_text: String,
    pub body_html: String,
    pub event: DetectionEvent,
}

impl PendingEmail {
    pub fn into_email(self) -> Email {
        Email {
            to: self.to,
            from: self.from,
            subject: self.subject,
            body_text: self.body_text,
            body_html: self.body_html,
        }
    }
}

fn backoff_seconds(retry_count: u32) -> i64 {
    match retry_count {
        0 => 60,
        1 => 120,
        2 => 300,
        3 => 600,
        4 => 1_800,
        5 => 3_600,
        6 => 10_800,
        _ => 21_600,
    }
}

#[derive(Debug)]
pub struct SmtpQueue {
    conn: rusqlite::Connection,
    retry_max: u32,
}

impl SmtpQueue {
    pub fn open<P: AsRef<Path>>(path: P, retry_max: u32) -> rusqlite::Result<Self> {
        let conn = rusqlite::Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS pending_emails (
                id INTEGER PRIMARY KEY,
                created_at INTEGER NOT NULL,
                retry_count INTEGER NOT NULL DEFAULT 0,
                next_attempt INTEGER NOT NULL,
                to_addr TEXT NOT NULL,
                from_addr TEXT NOT NULL,
                subject TEXT NOT NULL,
                body_text TEXT NOT NULL,
                body_html TEXT NOT NULL,
                event_json TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_pending_next ON pending_emails(next_attempt);
            CREATE INDEX IF NOT EXISTS idx_pending_retry ON pending_emails(retry_count);",
        )?;
        Ok(Self { conn, retry_max })
    }

    pub fn push(
        &mut self,
        email: &Email,
        event: &DetectionEvent,
        now: i64,
    ) -> Result<i64, SmtpError> {
        let json = serde_json::to_string(event)?;
        self.conn.execute(
            "INSERT INTO pending_emails
                (created_at, retry_count, next_attempt, to_addr, from_addr, subject, body_text, body_html, event_json)
             VALUES (?1, 0, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                now,
                now,
                email.to,
                email.from,
                email.subject,
                email.body_text,
                email.body_html,
                json,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn pop(&self, now: i64) -> Result<Option<PendingEmail>, SmtpError> {
        let row: Option<(i64, i64, i64, i64, String, String, String, String, String, String)> = self
            .conn
            .query_row(
                "SELECT id, created_at, retry_count, next_attempt, to_addr, from_addr, subject, body_text, body_html, event_json
                 FROM pending_emails
                 WHERE next_attempt <= ?1 AND retry_count < ?2
                 ORDER BY next_attempt ASC, id ASC
                 LIMIT 1",
                params![now, self.retry_max as i64],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                        r.get(9)?,
                    ))
                },
            )
            .optional()?;

        match row {
            Some((id, created, retries, next, to, from, subj, text, html, json)) => {
                let event: DetectionEvent = serde_json::from_str(&json)?;
                Ok(Some(PendingEmail {
                    id,
                    created_at: created,
                    retry_count: retries as u32,
                    next_attempt: next,
                    to,
                    from,
                    subject: subj,
                    body_text: text,
                    body_html: html,
                    event,
                }))
            }
            None => Ok(None),
        }
    }

    pub fn mark_done(&mut self, id: i64) -> Result<(), SmtpError> {
        self.conn
            .execute("DELETE FROM pending_emails WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn mark_retry(&mut self, id: i64, now: i64) -> Result<(), SmtpError> {
        let current: Option<i64> = self
            .conn
            .query_row(
                "SELECT retry_count FROM pending_emails WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(retries) = current else {
            return Ok(());
        };
        let retries = retries as u32;
        let new_retries = retries + 1;
        if new_retries >= self.retry_max {
            self.conn
                .execute("DELETE FROM pending_emails WHERE id = ?1", params![id])?;
        } else {
            let next = now + backoff_seconds(new_retries);
            self.conn.execute(
                "UPDATE pending_emails SET retry_count = ?1, next_attempt = ?2 WHERE id = ?3",
                params![new_retries, next, id],
            )?;
        }
        Ok(())
    }

    pub fn pending_count(&self) -> Result<usize, SmtpError> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM pending_emails", [], |r| r.get(0))?;
        Ok(n as usize)
    }

    pub fn next_attempt_for(&self, id: i64) -> Result<Option<i64>, SmtpError> {
        self.conn
            .query_row(
                "SELECT next_attempt FROM pending_emails WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventKind;

    fn sample_event() -> DetectionEvent {
        DetectionEvent {
            captured_at: 1000,
            kind: EventKind::DeviceJoined,
            pseudonym: "p1".into(),
            changed_fields: vec![],
            hostname: None,
            ip: None,
            mac: None,
            rssi_dbm: None,
            rcpi: None,
            band: None,
            channel: None,
            source: None,
            distance_m: None,
            connected: false,
            active: false,
            proximity: "Incerto".into(),
            heat: None,
            signal_quality: "N/A".into(),
            total_devices: 0,
            connected_count: 0,
            not_connected_count: 0,
        }
    }

    fn sample_email() -> Email {
        Email {
            to: "to@example.com".into(),
            from: "from@example.com".into(),
            subject: "test".into(),
            body_text: "text".into(),
            body_html: "html".into(),
        }
    }

    #[test]
    fn queue_persists_and_returns_email() {
        let mut q = SmtpQueue::open(":memory:", 8).unwrap();
        let id = q.push(&sample_email(), &sample_event(), 1).unwrap();
        assert_eq!(q.pending_count().unwrap(), 1);

        let p = q.pop(1).unwrap().unwrap();
        assert_eq!(p.id, id);
        assert_eq!(p.retry_count, 0);
        assert_eq!(p.event.pseudonym, "p1");
        assert_eq!(p.into_email(), sample_email());
    }

    #[test]
    fn pop_returns_only_when_ready() {
        let mut q = SmtpQueue::open(":memory:", 8).unwrap();
        q.push(&sample_email(), &sample_event(), 1000).unwrap();
        assert!(q.pop(0).unwrap().is_none());
        assert!(q.pop(1000).unwrap().is_some());
    }

    #[test]
    fn mark_done_removes_email() {
        let mut q = SmtpQueue::open(":memory:", 8).unwrap();
        let id = q.push(&sample_email(), &sample_event(), 1).unwrap();
        q.mark_done(id).unwrap();
        assert_eq!(q.pending_count().unwrap(), 0);
    }

    #[test]
    fn retry_scheduling_increases_backoff() {
        let mut q = SmtpQueue::open(":memory:", 8).unwrap();
        let id = q.push(&sample_email(), &sample_event(), 1).unwrap();
        q.mark_retry(id, 1).unwrap();
        let next = q.next_attempt_for(id).unwrap().unwrap();
        assert_eq!(next, 1 + 120);
        let p = q.pop(next).unwrap().unwrap();
        assert_eq!(p.retry_count, 1);
    }

    #[test]
    fn retry_deleted_after_max() {
        let mut q = SmtpQueue::open(":memory:", 2).unwrap();
        let id = q.push(&sample_email(), &sample_event(), 1).unwrap();
        q.mark_retry(id, 1).unwrap();
        assert_eq!(q.pending_count().unwrap(), 1);
        q.mark_retry(id, 1).unwrap();
        assert_eq!(q.pending_count().unwrap(), 0);
    }

    #[test]
    fn pop_respects_max_retries() {
        let mut q = SmtpQueue::open(":memory:", 1).unwrap();
        let id = q.push(&sample_email(), &sample_event(), 1).unwrap();
        q.mark_retry(id, 1).unwrap();
        assert!(q.pop(1).unwrap().is_none());
    }
}
