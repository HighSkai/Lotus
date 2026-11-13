//! LOTUS core: everything that does not touch the operating system.
//!
//! * [`model`] — server profiles and connection options
//! * [`import`] — parsers for share links, WireGuard and sing-box files
//! * [`validate`] — strict validation applied on both sides of the IPC boundary
//! * [`singbox`] — engine configuration builders
//! * [`ipc`] — the app ⇄ service protocol
//! * [`redact`] — log scrubbing

pub mod dnsmsg;
pub mod import;
pub mod ipc;
pub mod model;
pub mod providers;
pub mod redact;
pub mod singbox;
pub mod validate;

pub use model::*;
