use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::notifier::SmtpError;

fn parse_bool(v: &str) -> Option<bool> {
    match v.trim().to_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_u64(v: &str) -> Option<u64> {
    v.trim().parse().ok()
}

fn parse_u16(v: &str) -> Option<u16> {
    v.trim().parse().ok()
}

fn parse_u32(v: &str) -> Option<u32> {
    v.trim().parse().ok()
}

#[derive(Debug, Clone, PartialEq)]
pub struct SmtpConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from: String,
    pub to: String,
    pub starttls: bool,
    pub smtps: bool,
    pub connect_timeout: u64,
    pub send_timeout: u64,
    pub retry_max: u32,
    pub router_name: String,
    pub rate_joined: u64,
    pub rate_left: u64,
    pub rate_updated: u64,
    pub rate_nearby: u64,
}

impl Default for SmtpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: String::new(),
            port: 587,
            username: String::new(),
            password: String::new(),
            from: String::new(),
            to: String::new(),
            starttls: true,
            smtps: false,
            connect_timeout: 10,
            send_timeout: 30,
            retry_max: 8,
            router_name: "Detectic".into(),
            rate_joined: 600,
            rate_left: 600,
            rate_updated: 0,
            rate_nearby: 120,
        }
    }
}

impl SmtpConfig {
    pub fn from_env() -> Result<Self, SmtpError> {
        let mut m = HashMap::new();
        for (k, v) in std::env::vars() {
            if k.starts_with("SMTP_") || k.starts_with("DETECTIC_SMTP_") {
                m.insert(k, v);
            }
        }
        Self::from_values(&m)
    }

    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, SmtpError> {
        let content = fs::read_to_string(path).map_err(SmtpError::Io)?;
        let mut m = HashMap::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let key = k.trim().to_string();
                // Accept both SMTP_* and DETECTIC_SMTP_* prefixes
                if key.starts_with("SMTP_") || key.starts_with("DETECTIC_SMTP_") {
                    m.insert(key, v.trim().to_string());
                }
            }
        }
        Self::from_values(&m)
    }

    /// Resolve a config value: prefer `SMTP_*`, fall back to `DETECTIC_SMTP_*`.
    fn resolve<'a>(
        values: &'a HashMap<String, String>,
        smtp_key: &str,
        detectic_key: &'a str,
    ) -> Option<&'a str> {
        values
            .get(smtp_key)
            .or_else(|| values.get(detectic_key))
            .map(|s| s.as_str())
    }

    pub fn from_values(values: &HashMap<String, String>) -> Result<Self, SmtpError> {
        let mut c = Self::default();

        if let Some(v) = values.get("SMTP_ENABLED") {
            c.enabled = parse_bool(v)
                .ok_or_else(|| SmtpError::Config(format!("invalid SMTP_ENABLED: {v}")))?;
        }
        // SMTP_HOST ← SMTP_HOST | DETECTIC_SMTP_HOST
        if let Some(v) = Self::resolve(values, "SMTP_HOST", "DETECTIC_SMTP_HOST") {
            c.host = v.to_string();
        }
        // SMTP_PORT ← SMTP_PORT | DETECTIC_SMTP_PORT
        if let Some(v) = Self::resolve(values, "SMTP_PORT", "DETECTIC_SMTP_PORT") {
            c.port =
                parse_u16(v).ok_or_else(|| SmtpError::Config(format!("invalid SMTP_PORT: {v}")))?;
        }
        // SMTP_USERNAME ← SMTP_USERNAME | DETECTIC_SMTP_USER
        if let Some(v) = Self::resolve(values, "SMTP_USERNAME", "DETECTIC_SMTP_USER") {
            c.username = v.to_string();
        }
        // SMTP_PASSWORD ← SMTP_PASSWORD | DETECTIC_SMTP_PASSWORD
        if let Some(v) = Self::resolve(values, "SMTP_PASSWORD", "DETECTIC_SMTP_PASSWORD") {
            c.password = v.to_string();
        }
        // SMTP_FROM ← SMTP_FROM | DETECTIC_SMTP_FROM
        if let Some(v) = Self::resolve(values, "SMTP_FROM", "DETECTIC_SMTP_FROM") {
            c.from = v.to_string();
        }
        // SMTP_TO ← SMTP_TO | DETECTIC_SMTP_TO
        if let Some(v) = Self::resolve(values, "SMTP_TO", "DETECTIC_SMTP_TO") {
            c.to = v.to_string();
        }
        if let Some(v) = values.get("SMTP_STARTTLS") {
            c.starttls = parse_bool(v)
                .ok_or_else(|| SmtpError::Config(format!("invalid SMTP_STARTTLS: {v}")))?;
        }
        if let Some(v) = values.get("SMTP_SMTPS") {
            c.smtps = parse_bool(v)
                .ok_or_else(|| SmtpError::Config(format!("invalid SMTP_SMTPS: {v}")))?;
        }
        if let Some(v) = values.get("SMTP_CONNECT_TIMEOUT") {
            c.connect_timeout = parse_u64(v)
                .ok_or_else(|| SmtpError::Config(format!("invalid SMTP_CONNECT_TIMEOUT: {v}")))?;
        }
        if let Some(v) = values.get("SMTP_SEND_TIMEOUT") {
            c.send_timeout = parse_u64(v)
                .ok_or_else(|| SmtpError::Config(format!("invalid SMTP_SEND_TIMEOUT: {v}")))?;
        }
        if let Some(v) = values.get("SMTP_RETRY_MAX") {
            c.retry_max = parse_u32(v)
                .ok_or_else(|| SmtpError::Config(format!("invalid SMTP_RETRY_MAX: {v}")))?;
        }
        if let Some(v) = values.get("ROUTER_NAME") {
            c.router_name = v.clone();
        }
        if let Some(v) = values.get("SMTP_RATE_JOINED") {
            c.rate_joined = parse_u64(v)
                .ok_or_else(|| SmtpError::Config(format!("invalid SMTP_RATE_JOINED: {v}")))?;
        }
        if let Some(v) = values.get("SMTP_RATE_LEFT") {
            c.rate_left = parse_u64(v)
                .ok_or_else(|| SmtpError::Config(format!("invalid SMTP_RATE_LEFT: {v}")))?;
        }
        if let Some(v) = values.get("SMTP_RATE_UPDATED") {
            c.rate_updated = parse_u64(v)
                .ok_or_else(|| SmtpError::Config(format!("invalid SMTP_RATE_UPDATED: {v}")))?;
        }
        if let Some(v) = values.get("SMTP_RATE_NEARBY") {
            c.rate_nearby = parse_u64(v)
                .ok_or_else(|| SmtpError::Config(format!("invalid SMTP_RATE_NEARBY: {v}")))?;
        }

        if c.enabled && c.host.is_empty() {
            return Err(SmtpError::Config(
                "SMTP_HOST is required when SMTP_ENABLED=1".into(),
            ));
        }
        if c.enabled && c.from.is_empty() {
            return Err(SmtpError::Config(
                "SMTP_FROM is required when SMTP_ENABLED=1".into(),
            ));
        }
        if c.enabled && c.to.is_empty() {
            return Err(SmtpError::Config(
                "SMTP_TO is required when SMTP_ENABLED=1".into(),
            ));
        }
        if c.enabled && c.username.is_empty() != c.password.is_empty() {
            return Err(SmtpError::Config(
                "SMTP_USERNAME and SMTP_PASSWORD must both be set or both empty".into(),
            ));
        }

        Ok(c)
    }

    pub fn smtp_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.send_timeout)
    }

    pub fn connect_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.connect_timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_disabled() {
        let c = SmtpConfig::default();
        assert!(!c.enabled);
        assert_eq!(c.port, 587);
        assert!(c.starttls);
    }

    #[test]
    fn config_from_values() {
        let mut m = HashMap::new();
        m.insert("SMTP_ENABLED".into(), "1".into());
        m.insert("SMTP_HOST".into(), "smtp.example.com".into());
        m.insert("SMTP_PORT".into(), "587".into());
        m.insert("SMTP_USERNAME".into(), "alerts@example.com".into());
        m.insert("SMTP_PASSWORD".into(), "secret".into());
        m.insert("SMTP_FROM".into(), "alerts@example.com".into());
        m.insert("SMTP_TO".into(), "security@example.com".into());
        m.insert("SMTP_STARTTLS".into(), "1".into());
        m.insert("SMTP_RETRY_MAX".into(), "5".into());
        m.insert("ROUTER_NAME".into(), "EX520".into());

        let c = SmtpConfig::from_values(&m).unwrap();
        assert!(c.enabled);
        assert_eq!(c.host, "smtp.example.com");
        assert_eq!(c.port, 587);
        assert_eq!(c.password, "secret");
        assert_eq!(c.router_name, "EX520");
        assert_eq!(c.retry_max, 5);
    }

    #[test]
    fn enabled_config_requires_host() {
        let mut m = HashMap::new();
        m.insert("SMTP_ENABLED".into(), "1".into());
        let e = SmtpConfig::from_values(&m).unwrap_err();
        match e {
            SmtpError::Config(_) => {}
            _ => panic!("expected config error"),
        }
    }

    #[test]
    fn config_file_parses() {
        use std::io::Write;

        let mut tmp = std::env::temp_dir();
        tmp.push("detectic_smtp_test.conf");
        let mut f = std::fs::File::create(&tmp).unwrap();
        writeln!(f, "# comment").unwrap();
        writeln!(f, "SMTP_ENABLED=1").unwrap();
        writeln!(f, "SMTP_HOST=smtp.example.com").unwrap();
        writeln!(f, "SMTP_FROM=a@b.com").unwrap();
        writeln!(f, "SMTP_TO=c@d.com").unwrap();
        drop(f);

        let c = SmtpConfig::from_file(&tmp).unwrap();
        assert!(c.enabled);
        assert_eq!(c.host, "smtp.example.com");

        std::fs::remove_file(&tmp).unwrap();
    }

    #[test]
    fn detectic_smtp_prefix_fallback() {
        let mut m = HashMap::new();
        m.insert("SMTP_ENABLED".into(), "1".into());
        m.insert("DETECTIC_SMTP_HOST".into(), "smtp-relay.brevo.com".into());
        m.insert("DETECTIC_SMTP_PORT".into(), "587".into());
        m.insert("DETECTIC_SMTP_USER".into(), "user@gmail.com".into());
        m.insert("DETECTIC_SMTP_PASSWORD".into(), "xsmtpsib-test".into());
        m.insert("DETECTIC_SMTP_FROM".into(), "Bot <bot@example.com>".into());
        m.insert("DETECTIC_SMTP_TO".into(), "admin@example.com".into());

        let c = SmtpConfig::from_values(&m).unwrap();
        assert!(c.enabled);
        assert_eq!(c.host, "smtp-relay.brevo.com");
        assert_eq!(c.port, 587);
        assert_eq!(c.username, "user@gmail.com");
        assert_eq!(c.password, "xsmtpsib-test");
        assert_eq!(c.from, "Bot <bot@example.com>");
        assert_eq!(c.to, "admin@example.com");
    }

    #[test]
    fn smtp_prefix_wins_over_detectic_prefix() {
        let mut m = HashMap::new();
        m.insert("SMTP_HOST".into(), "smtp.primary.com".into());
        m.insert("DETECTIC_SMTP_HOST".into(), "smtp.fallback.com".into());
        m.insert("SMTP_USERNAME".into(), "user@primary.com".into());
        m.insert("DETECTIC_SMTP_USER".into(), "user@fallback.com".into());

        let c = SmtpConfig::from_values(&m).unwrap();
        assert_eq!(c.host, "smtp.primary.com");
        assert_eq!(c.username, "user@primary.com");
    }
}
