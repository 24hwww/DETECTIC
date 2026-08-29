use std::cell::RefCell;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::Arc;

use base64::Engine as _;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use crate::notifier::{
    DetectionEvent, Email, EmailTemplate, Notifier, RateLimiter, SmtpConfig, SmtpError, SmtpQueue,
};

pub trait SmtpTransport {
    fn send_email(&self, email: &Email) -> Result<(), SmtpError>;
}

pub struct RustlsSmtpTransport {
    smtp: SmtpConfig,
    client_config: Arc<ClientConfig>,
    server_name: ServerName<'static>,
}

impl RustlsSmtpTransport {
    pub fn new(config: &SmtpConfig) -> Result<Self, SmtpError> {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let root_store = RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS
                .iter()
                .map(|ta| ta.to_owned())
                .collect(),
        };

        let client_config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let server_name =
            ServerName::try_from(config.host.clone()).map_err(|_| SmtpError::InvalidHost)?;

        Ok(Self {
            smtp: config.clone(),
            client_config: Arc::new(client_config),
            server_name,
        })
    }

    fn addr(&self) -> Result<std::net::SocketAddr, SmtpError> {
        use std::net::ToSocketAddrs;
        let addr = (self.smtp.host.as_str(), self.smtp.port)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| SmtpError::InvalidHost)?;
        Ok(addr)
    }

    fn read_line<B: BufRead>(reader: &mut B, line: &mut String) -> Result<(), SmtpError> {
        line.clear();
        reader.read_line(line)?;
        Ok(())
    }

    fn read_response<B: BufRead>(reader: &mut B) -> Result<(u16, String), SmtpError> {
        let mut first = String::new();
        Self::read_line(reader, &mut first)?;
        if first.len() < 4 {
            return Err(SmtpError::Smtp("short smtp response".into()));
        }
        let code: u16 = first[..3]
            .parse()
            .map_err(|_| SmtpError::Smtp("invalid smtp response code".into()))?;
        let mut full = first.clone();
        let mut more = &first[3..4] == "-";
        while more {
            let mut line = String::new();
            Self::read_line(reader, &mut line)?;
            if line.len() < 4 {
                break;
            }
            more = &line[3..4] == "-";
            full.push_str(&line);
        }
        if code >= 400 {
            return Err(SmtpError::Smtp(full.trim().into()));
        }
        Ok((code, full))
    }

    fn send_line<W: Write>(writer: &mut W, line: &str) -> Result<(), SmtpError> {
        writer.write_all(line.as_bytes())?;
        writer.write_all(b"\r\n")?;
        writer.flush()?;
        Ok(())
    }

    fn ehlo<R: Read + Write>(&self, reader: &mut BufReader<&mut R>) -> Result<(), SmtpError> {
        Self::send_line(reader.get_mut(), "EHLO detectic")?;
        Self::read_response(reader)?;
        Ok(())
    }

    fn helo<R: Read + Write>(&self, reader: &mut BufReader<&mut R>) -> Result<(), SmtpError> {
        Self::send_line(reader.get_mut(), "HELO detectic")?;
        Self::read_response(reader)?;
        Ok(())
    }

    fn auth<R: Read + Write>(&self, reader: &mut BufReader<&mut R>) -> Result<(), SmtpError> {
        if self.smtp.username.is_empty() {
            return Ok(());
        }
        let mut creds = String::new();
        creds.push('\0');
        creds.push_str(&self.smtp.username);
        creds.push('\0');
        creds.push_str(&self.smtp.password);
        let encoded = base64::engine::general_purpose::STANDARD.encode(creds.as_bytes());
        Self::send_line(reader.get_mut(), &format!("AUTH PLAIN {encoded}"))?;
        Self::read_response(reader)?;
        Ok(())
    }

    fn boundary() -> String {
        format!(
            "----=_Boundary_{}",
            std::time::UNIX_EPOCH
                .elapsed()
                .unwrap_or_default()
                .as_nanos()
        )
    }

    fn mime_body(&self, email: &Email) -> String {
        let b = Self::boundary();
        format!(
            "From: {}\r\n\
             To: {}\r\n\
             Subject: {}\r\n\
             MIME-Version: 1.0\r\n\
             Content-Type: multipart/alternative; boundary=\"{}\"\r\n\
             \r\n\
             --{}\r\n\
             Content-Type: text/plain; charset=\"utf-8\"\r\n\
             Content-Transfer-Encoding: 8bit\r\n\
             \r\n\
             {}\r\n\
             --{}\r\n\
             Content-Type: text/html; charset=\"utf-8\"\r\n\
             Content-Transfer-Encoding: 8bit\r\n\
             \r\n\
             {}\r\n\
             --{}--\r\n",
            email.from, email.to, email.subject, b, b, email.body_text, b, email.body_html, b
        )
    }

    /// Extract the bare email address from a possibly display-name-wrapped
    /// value like `"Womni-bot <bot@e-mail.womni.com.br>"` → `"bot@e-mail.womni.com.br"`.
    fn extract_addr(s: &str) -> &str {
        if let Some(start) = s.find('<') {
            if let Some(end) = s.find('>') {
                return &s[start + 1..end];
            }
        }
        s
    }

    fn send_message<R: Read + Write>(
        &self,
        reader: &mut BufReader<&mut R>,
        email: &Email,
    ) -> Result<(), SmtpError> {
        let from_addr = Self::extract_addr(&email.from);
        Self::send_line(reader.get_mut(), &format!("MAIL FROM:<{}>", from_addr))?;
        Self::read_response(reader)?;
        // RCPT TO may contain comma-separated addresses
        for addr in email.to.split(',') {
            let addr = addr.trim();
            if addr.is_empty() {
                continue;
            }
            let rcpt = Self::extract_addr(addr);
            Self::send_line(reader.get_mut(), &format!("RCPT TO:<{}>", rcpt))?;
            Self::read_response(reader)?;
        }
        Self::send_line(reader.get_mut(), "DATA")?;
        Self::read_response(reader)?;
        let body = self.mime_body(email);
        reader.get_mut().write_all(body.as_bytes())?;
        reader.get_mut().write_all(b"\r\n.\r\n")?;
        reader.get_mut().flush()?;
        Self::read_response(reader)?;
        Ok(())
    }

    fn send_smtps(&self, email: &Email, sock: TcpStream) -> Result<(), SmtpError> {
        let conn = ClientConnection::new(self.client_config.clone(), self.server_name.clone())?;
        let mut stream = StreamOwned::new(conn, sock);
        let mut reader = BufReader::new(&mut stream);
        Self::read_response(&mut reader)?;
        self.ehlo(&mut reader)?;
        self.auth(&mut reader)?;
        self.send_message(&mut reader, email)?;
        let _ = Self::send_line(reader.get_mut(), "QUIT");
        Ok(())
    }

    fn send_starttls(
        &self,
        email: &Email,
        maybe_sock: &mut Option<TcpStream>,
    ) -> Result<(), SmtpError> {
        let mut sock = maybe_sock
            .take()
            .ok_or_else(|| SmtpError::Smtp("socket already consumed".into()))?;
        let mut reader = BufReader::new(&mut sock);
        Self::read_response(&mut reader)?;
        self.ehlo(&mut reader)?;
        Self::send_line(reader.get_mut(), "STARTTLS")?;
        Self::read_response(&mut reader)?;
        drop(reader);

        let conn = ClientConnection::new(self.client_config.clone(), self.server_name.clone())?;
        let mut stream = StreamOwned::new(conn, sock);
        let mut reader = BufReader::new(&mut stream);
        self.ehlo(&mut reader)?;
        self.auth(&mut reader)?;
        self.send_message(&mut reader, email)?;
        let _ = Self::send_line(reader.get_mut(), "QUIT");
        Ok(())
    }

    fn send_plain(&self, email: &Email, sock: TcpStream) -> Result<(), SmtpError> {
        let mut stream = sock;
        let mut reader = BufReader::new(&mut stream);
        Self::read_response(&mut reader)?;
        self.helo(&mut reader)?;
        self.auth(&mut reader)?;
        self.send_message(&mut reader, email)?;
        let _ = Self::send_line(reader.get_mut(), "QUIT");
        Ok(())
    }
}

impl SmtpTransport for RustlsSmtpTransport {
    fn send_email(&self, email: &Email) -> Result<(), SmtpError> {
        let addr = self.addr()?;
        let sock = TcpStream::connect_timeout(&addr, self.smtp.connect_timeout())?;
        sock.set_read_timeout(Some(self.smtp.smtp_timeout()))?;
        sock.set_write_timeout(Some(self.smtp.smtp_timeout()))?;

        if self.smtp.smtps {
            self.send_smtps(email, sock)
        } else if self.smtp.starttls {
            let mut maybe = Some(sock);
            let r = self.send_starttls(email, &mut maybe);
            if let Some(s) = maybe {
                drop(s);
            }
            r
        } else {
            self.send_plain(email, sock)
        }
    }
}

pub struct SmtpNotifier {
    config: SmtpConfig,
    queue: RefCell<SmtpQueue>,
    rate: RefCell<RateLimiter>,
    transport: Box<dyn SmtpTransport>,
}

impl SmtpNotifier {
    pub fn new<P: AsRef<Path>>(
        config: SmtpConfig,
        queue_path: P,
        transport: Box<dyn SmtpTransport>,
    ) -> Result<Self, SmtpError> {
        let queue = SmtpQueue::open(queue_path, config.retry_max)?;
        let rate_cfg = (&config).into();
        Ok(Self {
            config,
            queue: RefCell::new(queue),
            rate: RefCell::new(RateLimiter::new(rate_cfg)),
            transport,
        })
    }

    pub fn flush(&self) -> Result<u32, SmtpError> {
        let now = chrono::Utc::now().timestamp();
        let mut sent = 0u32;
        let mut last_err = None;
        loop {
            let pending = self.queue.borrow().pop(now)?;
            let Some(pending) = pending else { break };
            let id = pending.id;
            let email = pending.into_email();
            match self.transport.send_email(&email) {
                Ok(()) => {
                    self.queue.borrow_mut().mark_done(id)?;
                    sent = sent.saturating_add(1);
                }
                Err(e) => {
                    self.queue.borrow_mut().mark_retry(id, now)?;
                    last_err = Some(e);
                }
            }
        }
        match last_err {
            Some(e) => Err(e),
            None => Ok(sent),
        }
    }
}

impl Notifier for SmtpNotifier {
    fn send(&self, event: &DetectionEvent) -> Result<(), SmtpError> {
        if !self.config.enabled {
            return Ok(());
        }

        let now = chrono::Utc::now().timestamp();
        if !self
            .rate
            .borrow_mut()
            .record(&event.pseudonym, &event.kind, now)
        {
            return Ok(());
        }

        let (subject, text, html) = EmailTemplate::render(event, &self.config);
        let email = Email {
            to: self.config.to.clone(),
            from: self.config.from.clone(),
            subject,
            body_text: text,
            body_html: html,
        };

        self.queue.borrow_mut().push(&email, event, now)?;
        Ok(())
    }
}

#[cfg(test)]
pub struct MockSmtpTransport {
    pub sent: std::sync::Mutex<Vec<Email>>,
    fail: std::sync::atomic::AtomicBool,
}

#[cfg(test)]
impl MockSmtpTransport {
    pub fn new() -> Self {
        Self {
            sent: std::sync::Mutex::new(Vec::new()),
            fail: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn failing() -> Self {
        Self {
            sent: std::sync::Mutex::new(Vec::new()),
            fail: std::sync::atomic::AtomicBool::new(true),
        }
    }
}

#[cfg(test)]
impl SmtpTransport for MockSmtpTransport {
    fn send_email(&self, email: &Email) -> Result<(), SmtpError> {
        self.sent.lock().unwrap().push(email.clone());
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            Err(SmtpError::Smtp("mock failure".into()))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventKind;

    fn event() -> DetectionEvent {
        DetectionEvent {
            captured_at: 1,
            kind: EventKind::DeviceJoined,
            pseudonym: "p1".into(),
            changed_fields: vec![],
            hostname: Some("phone".into()),
            ip: Some("10.0.0.1".into()),
            mac: Some("AA:BB:CC:DD:EE:FF".into()),
            rssi_dbm: None,
            rcpi: Some(104),
            band: Some("2.4G".into()),
            channel: Some(6),
            source: Some("wifi".into()),
            distance_m: Some(2.5),
            connected: true,
            active: true,
            proximity: "Perto".into(),
            heat: Some(75),
            signal_quality: "Bom".into(),
            total_devices: 5,
            connected_count: 5,
            not_connected_count: 0,
        }
    }

    fn enabled_config() -> SmtpConfig {
        SmtpConfig {
            enabled: true,
            host: "smtp.example.com".into(),
            port: 587,
            username: "alerts@example.com".into(),
            password: "secret".into(),
            from: "alerts@example.com".into(),
            to: "security@example.com".into(),
            starttls: true,
            smtps: false,
            ..SmtpConfig::default()
        }
    }

    #[test]
    fn smtp_notifies_and_queues() {
        let config = enabled_config();
        let transport = Box::new(MockSmtpTransport::new());
        let n = SmtpNotifier::new(config, ":memory:", transport).unwrap();
        n.send(&event()).unwrap();
        assert_eq!(n.queue.borrow().pending_count().unwrap(), 1);
        let sent = n.flush().unwrap();
        assert_eq!(sent, 1);
        assert_eq!(n.queue.borrow().pending_count().unwrap(), 0);
    }

    #[test]
    fn rate_limiter_suppresses_spam() {
        let config = enabled_config();
        let transport = Box::new(MockSmtpTransport::new());
        let n = SmtpNotifier::new(config, ":memory:", transport).unwrap();
        n.send(&event()).unwrap();
        n.send(&event()).unwrap();
        assert_eq!(n.queue.borrow().pending_count().unwrap(), 1);
    }

    #[test]
    fn disabled_smtp_is_silent() {
        let mut config = enabled_config();
        config.enabled = false;
        let transport = Box::new(MockSmtpTransport::new());
        let n = SmtpNotifier::new(config, ":memory:", transport).unwrap();
        n.send(&event()).unwrap();
        assert_eq!(n.queue.borrow().pending_count().unwrap(), 0);
    }

    #[test]
    fn failed_send_schedules_retry() {
        let config = enabled_config();
        let transport = Box::new(MockSmtpTransport::failing());
        let n = SmtpNotifier::new(config, ":memory:", transport).unwrap();
        n.send(&event()).unwrap();
        let _ = n.flush();
        assert_eq!(n.queue.borrow().pending_count().unwrap(), 1);
        let p = n.queue.borrow().pop(i64::MAX).unwrap().unwrap();
        assert_eq!(p.retry_count, 1);
    }
}
