# Detectic — SMTP Notification Design (Milestone M5)

## 1. Goal

Add an independent, embedded SMTP notification channel to Detectic that runs
on the stock EX520V and sends privacy-safe email alerts for device-lifecycle
events.  It must not modify the router firmware and must survive network
outages.

## 2. Architecture

```text
WiFi Provider  →  Collector  →  Detection Engine  →  Event Queue
                                                          │
                                                          ▼
                                               ┌──────────────────┐
                                               │   Notifier       │
                                               │  (SmtpNotifier)  │
                                               └────────┬─────────┘
                                                        │
                                          ┌─────────────┴─────────────┐
                                          ▼                             ▼
                                    SmtpQueue (SQLite)          NullNotifier (tests)
                                          │
                                          ▼
                              RustlsSmtpTransport (STARTTLS/SMTPS)
                                          │
                                          ▼
                                     SMTP server
```

The existing `UploadQueue` to the backend is untouched.  SMTP is an additional,
optional notification channel.

## 3. Module layout

| File | Responsibility |
|---|---|
| `src/notifier/mod.rs` | Public API, `DetectionEvent`, `Notifier` trait, `SmtpError`, `NullNotifier` |
| `src/notifier/config.rs` | `SmtpConfig` parsing from environment variables or `*.conf` files |
| `src/notifier/rate_limit.rs` | Per-device/per-event rate limiting |
| `src/notifier/queue.rs` | Persistent SQLite queue with retry counters and backoff |
| `src/notifier/template.rs` | Plain-text and HTML email generation, MAC masking, RCPI → dBm |
| `src/notifier/smtp.rs` | `SmtpTransport` trait, `RustlsSmtpTransport`, `SmtpNotifier`, `MockSmtpTransport` |

## 4. Configuration

Configuration is loaded at runtime from `SMTP_*` environment variables or from a
`KEY=value` file.  No credentials are compiled into the binary.

`router/detectic.conf.example` documents every supported key.

Sensitive keys (`SMTP_PASSWORD`) are read but never logged or displayed.

## 5. Queue and retry flow

1. `SmtpNotifier::send()` validates the event against the rate limiter.
2. If allowed, the email is rendered and persisted in `SmtpQueue`.
3. `SmtpNotifier::flush()` pops ready items and attempts delivery.
4. On success: the row is deleted.
5. On failure: `retry_count` is incremented and `next_attempt` is advanced by
   the exponential backoff table.
6. When `retry_count` reaches `SMTP_RETRY_MAX`, the row is discarded.

Backoff table (seconds):

| retry | wait |
|---|---|
| 0→1 | 60 |
| 1→2 | 120 |
| 2→3 | 300 |
| 3→4 | 600 |
| 4→5 | 1 800 |
| 5→6 | 3 600 |
| 6→7 | 10 800 |
| 7→8 | 21 600 |

## 6. Security model

- No credentials in source code or version control.
- `SMTP_PASSWORD` is never logged.
- Only the masked MAC is placed in the email body.
- TLS certificate validation is mandatory; invalid certificates fail closed.
- STARTTLS and SMTPS use `rustls` with `ring` and `webpki-roots`.
- Plain-text SMTP (no TLS) is supported only for explicitly trusted relays.

## 7. Email content

Each message is sent as `multipart/alternative` with `text/plain` and
`text/html` parts.  The HTML template is lightweight, responsive, and contains
no images.

MACs are masked as `AA:BB:CC:**:**:EE:FF` by default.

## 8. Rate limiting

Defaults per `EventKind`:

| Event | Window | Default |
|---|---|---|
| `DEVICE_JOINED` | 600 s | enabled |
| `DEVICE_LEFT` | 600 s | enabled |
| `DEVICE_UPDATED` | disabled | 0 s |
| `DEVICE_NEARBY` | 120 s | enabled but only for future events |

A rate-limited event is silently dropped; the queue is not flooded.

## 9. Provider notes

### Generic SMTP

Use `SMTP_HOST`, `SMTP_PORT`, `SMTP_USERNAME`, `SMTP_PASSWORD`.
Set `SMTP_STARTTLS=1` for port 587 or `SMTP_SMTPS=1` for port 465.

### Gmail

- Host: `smtp.gmail.com`
- Port: 587 with `SMTP_STARTTLS=1`
- Username: full Gmail address
- Password: use an **App Password**, not the web password.
- `SMTP_FROM` should match the Gmail account.

### Outlook / Microsoft 365

- Host: `smtp-mail.outlook.com` or `smtp.office365.com`
- Port: 587 with `SMTP_STARTTLS=1`
- Use an **App Password** if 2FA is enabled.

### Zoho Mail

- Host: `smtp.zoho.com`
- Port: 587 with `SMTP_STARTTLS=1`
- Use an **App Password**.

## 10. Testing

All network calls are mocked.  `MockSmtpTransport` records sent emails.
`NullNotifier` is used for collector tests.  Real SMTP connections are never
made during `cargo test`.

## 11. Future work

- `DEVICE_NEARBY` and `DEVICE_PROXIMITY` event integration once the detection
  engine supports them.
- Per-provider autoconfiguration from MX records or autodiscover.
- DSN / read-receipt tracking for delivery confirmation.
