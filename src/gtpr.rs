//! Compatibility shim — the GTPR implementation now lives in
//! `transport` (raw GDPR session) and `collector` (OID merge).
//! This module re-exports the public API so `use detectic::gtpr::*` keeps
//! working and existing tests continue to pass.

pub use crate::collector::{canon_mac, collect, parse_network_map};
pub use crate::transport::{Dialect, GtprClient, GtprError, Transport};
