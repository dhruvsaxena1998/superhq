//! Wire protocol for SuperHQ remote control.
//!
//! JSON-RPC 2.0 envelope over newline-delimited JSON, carried on iroh
//! QUIC streams. PTY data streams bypass this envelope and carry raw bytes
//! directly.

pub mod envelope;
pub mod methods;
pub mod notifications;
pub mod stream;
pub mod types;

pub use envelope::*;

/// ALPN identifier for the SuperHQ remote protocol.
pub const ALPN: &[u8] = b"superhq/remote/1";

/// Highest protocol version this build understands.
pub const PROTOCOL_VERSION: u32 = 1;
