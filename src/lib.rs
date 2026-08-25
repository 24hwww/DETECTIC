//! Detectic: TP-Link EX520 network-map sensor with local persistence.
//!
//! Library entry point. The binary (`src/main.rs`) wires the GTPR client to the
//! SQLite store; embedding code should do the same.

pub mod analytics;
pub mod backend;
pub mod calibrate;
pub mod collector;
pub mod config;
pub mod crypto;
pub mod driver;
pub mod events;
pub mod fusion;
pub mod gtpr;
pub mod http;
pub mod launcher;
pub mod logging;
pub mod model;
pub mod monitor;
#[cfg(feature = "persist")]
pub mod notifier;
pub mod oids;
pub mod persistence;
pub mod presence;
pub mod publisher;
pub mod realtime;
pub mod runtime;
pub mod service;
pub mod snapshot;
#[cfg(feature = "persist")]
pub mod store;
pub mod transport;

pub use crypto::*;
pub use gtpr::{Dialect, GtprClient};
pub use model::{Device, MapDiff, NetworkMap};
#[cfg(feature = "persist")]
pub use store::Store;
// Layered architecture (Phase B): transport / collector / publisher
pub use collector::{collect, parse_network_map};
pub use transport::{
    Dialect as TransportDialect, GtprClient as TransportClient, GtprError, Transport,
};
