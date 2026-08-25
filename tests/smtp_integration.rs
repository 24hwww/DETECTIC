//! Integration test: send a real email via Brevo SMTP relay.
//!
//! Run with:
//!   cargo test --features persist smtp_integration -- --ignored --nocapture
//!
//! Requires network access to smtp-relay.brevo.com:587.

#![cfg(feature = "persist")]

use detectic::notifier::{
    DetectionEvent, EmailTemplate, Notifier, SmtpConfig, SmtpNotifier,
    RustlsSmtpTransport,
};
use detectic::events::EventKind;

fn brevo_config() -> SmtpConfig {
    let mut m = std::collections::HashMap::new();
    m.insert("SMTP_ENABLED".into(), "1".into());
    m.insert("DETECTIC_SMTP_HOST".into(), "smtp-relay.brevo.com".into());
    m.insert("DETECTIC_SMTP_PORT".into(), "587".into());
    m.insert("DETECTIC_SMTP_USER".into(), "24hwww@gmail.com".into());
    m.insert(
        "DETECTIC_SMTP_PASSWORD".into(),
        "CHANGE_ME_SMTP".into(),
    );
    m.insert(
        "DETECTIC_SMTP_FROM".into(),
        "Womni-bot <bot@e-mail.womni.com.br>".into(),
    );
    m.insert(
        "DETECTIC_SMTP_TO".into(),
        "24hwww+detectic@gmail.com,natasthefany+detectic@gmail.com".into(),
    );
    m.insert("SMTP_STARTTLS".into(), "1".into());
    m.insert("SMTP_RETRY_MAX".into(), "3".into());
    m.insert("ROUTER_NAME".into(), "EX520-Test".into());
    // Rate=0 means DISABLED; use 1 second for testing (minimal rate limit)
    m.insert("SMTP_RATE_JOINED".into(), "1".into());
    m.insert("SMTP_RATE_LEFT".into(), "1".into());
    m.insert("SMTP_RATE_UPDATED".into(), "1".into());
    m.insert("SMTP_RATE_NEARBY".into(), "1".into());
    SmtpConfig::from_values(&m).unwrap()
}

fn sample_event(kind: EventKind, pseudonym: &str) -> DetectionEvent {
    DetectionEvent {
        captured_at: chrono::Utc::now().timestamp(),
        kind,
        pseudonym: pseudonym.into(),
        changed_fields: vec![],
        hostname: Some("test-phone".into()),
        ip: Some("192.168.0.42".into()),
        mac: Some("AA:BB:CC:DD:EE:FF".into()),
        rssi_dbm: Some(-55),
        rcpi: None,
        band: Some("2.4GHz".into()),
        channel: Some(6),
        source: Some("wifi".into()),
        distance_m: Some(2.5),
        connected: true,
        active: true,
        proximity: "Perto".into(),
        signal_quality: "Bom".into(),
        total_devices: 5,
        connected_count: 5,
        not_connected_count: 0,
    }
}

#[test]
#[ignore] // run with: cargo test --features persist smtp_integration -- --ignored --nocapture
fn send_device_joined_email_via_brevo() {
    let config = brevo_config();
    let transport = Box::new(RustlsSmtpTransport::new(&config).expect("tls transport"));
    let queue_path = std::env::temp_dir().join("detectic_smtp_test_queue.db");
    let _ = std::fs::remove_file(&queue_path);
    let notifier = SmtpNotifier::new(config.clone(), &queue_path, transport)
        .expect("notifier created");

    let event = sample_event(EventKind::DeviceJoined, "test-pseudo-aaa111");
    println!("[test] sending DeviceJoined email...");
    notifier.send(&event).expect("send queued");

    let sent = notifier.flush().expect("flush");
    println!("[test] flushed {} email(s)", sent);
    assert!(sent >= 1, "at least one email should have been sent");

    // Send a DeviceLeft event too
    let event2 = sample_event(EventKind::DeviceLeft, "test-pseudo-bbb222");
    println!("[test] sending DeviceLeft email...");
    notifier.send(&event2).expect("send queued");
    let sent2 = notifier.flush().expect("flush");
    println!("[test] flushed {} email(s)", sent2);
    assert!(sent2 >= 1);

    // Send a DeviceUpdated event (rate=0 should allow it)
    let mut event3 = sample_event(EventKind::DeviceUpdated, "test-pseudo-ccc333");
    event3.changed_fields = vec!["rssi".into()];
    println!("[test] sending DeviceUpdated email...");
    notifier.send(&event3).expect("send queued");
    let sent3 = notifier.flush().expect("flush");
    println!("[test] flushed {} email(s)", sent3);

    let _ = std::fs::remove_file(&queue_path);
    println!("[test] all emails sent successfully via Brevo SMTP");
}

#[test]
#[ignore]
fn render_email_template_preview() {
    let config = brevo_config();
    let event = sample_event(EventKind::DeviceJoined, "preview-pseudo");
    let (subject, text, html) = EmailTemplate::render(&event, &config);
    println!("=== SUBJECT ===\n{}\n", subject);
    println!("=== TEXT ===\n{}\n", text);
    println!("=== HTML (first 500 chars) ===\n{}\n", &html[..html.len().min(500)]);
    assert!(!subject.is_empty());
    assert!(text.contains("EX520-Test"));
    assert!(text.contains("AA:BB:CC:**:**:EE:FF")); // masked MAC
}
