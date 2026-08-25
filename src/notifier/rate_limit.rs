use std::collections::HashMap;

use crate::events::EventKind;
use crate::notifier::SmtpConfig;

#[derive(Debug, Clone, PartialEq)]
pub struct RateLimitConfig {
    pub joined: u64,
    pub left: u64,
    pub updated: u64,
    pub nearby: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            joined: 600,
            left: 600,
            updated: 0,
            nearby: 120,
        }
    }
}

impl From<&SmtpConfig> for RateLimitConfig {
    fn from(c: &SmtpConfig) -> Self {
        Self {
            joined: c.rate_joined,
            left: c.rate_left,
            updated: c.rate_updated,
            nearby: c.rate_nearby,
        }
    }
}

#[derive(Debug, Default)]
pub struct RateLimiter {
    cfg: RateLimitConfig,
    next_allowed: HashMap<(String, EventKind), i64>,
}

impl RateLimiter {
    pub fn new(cfg: RateLimitConfig) -> Self {
        Self {
            cfg,
            next_allowed: HashMap::new(),
        }
    }

    pub fn window_for(&self, kind: &EventKind) -> Option<u64> {
        match kind {
            EventKind::DeviceJoined => Some(self.cfg.joined).filter(|&w| w > 0),
            EventKind::DeviceLeft => Some(self.cfg.left).filter(|&w| w > 0),
            EventKind::DeviceUpdated => Some(self.cfg.updated).filter(|&w| w > 0),
        }
    }

    pub fn is_allowed(&self, pseudonym: &str, kind: &EventKind, now: i64) -> bool {
        if self.window_for(kind).is_none() {
            return false;
        }
        let key = (pseudonym.to_string(), kind.clone());
        match self.next_allowed.get(&key) {
            Some(t) if now < *t => false,
            _ => true,
        }
    }

    pub fn record(&mut self, pseudonym: &str, kind: &EventKind, now: i64) -> bool {
        if !self.is_allowed(pseudonym, kind, now) {
            return false;
        }
        let Some(window) = self.window_for(kind) else {
            return false;
        };
        let key = (pseudonym.to_string(), kind.clone());
        self.next_allowed.insert(key, now + window as i64);
        true
    }

    pub fn reset(&mut self, pseudonym: &str, kind: &EventKind) {
        self.next_allowed
            .remove(&(pseudonym.to_string(), kind.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_event_allowed() {
        let mut r = RateLimiter::new(RateLimitConfig::default());
        assert!(r.record("p1", &EventKind::DeviceJoined, 1000));
    }

    #[test]
    fn same_event_inside_window_is_blocked() {
        let mut r = RateLimiter::new(RateLimitConfig {
            joined: 100,
            left: 100,
            updated: 0,
            nearby: 0,
        });
        assert!(r.record("p1", &EventKind::DeviceJoined, 0));
        assert!(!r.record("p1", &EventKind::DeviceJoined, 50));
        assert!(!r.is_allowed("p1", &EventKind::DeviceJoined, 50));
        assert!(r.record("p1", &EventKind::DeviceJoined, 100));
    }

    #[test]
    fn different_devices_are_independent() {
        let mut r = RateLimiter::new(RateLimitConfig::default());
        assert!(r.record("p1", &EventKind::DeviceJoined, 0));
        assert!(r.record("p2", &EventKind::DeviceJoined, 0));
    }

    #[test]
    fn updated_is_disabled_by_default() {
        let mut r = RateLimiter::new(RateLimitConfig::default());
        assert!(!r.record("p1", &EventKind::DeviceUpdated, 0));
    }

    #[test]
    fn nearby_future_kind_is_disabled_by_default() {
        let r = RateLimiter::new(RateLimitConfig::default());
        assert_eq!(r.window_for(&EventKind::DeviceJoined), Some(600));
        assert_eq!(r.window_for(&EventKind::DeviceUpdated), None);
    }
}
