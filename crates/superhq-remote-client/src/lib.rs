//! Client-side transport for SuperHQ remote control.
//!
//! `RemoteClient` connects to a host's iroh `EndpointId` via the
//! `superhq/remote/1` ALPN, drives the control-stream JSON-RPC loop, and
//! provides typed RPC methods plus primitives for opening PTY data streams.
//!
//! Compiles natively (used by tests) and to WASM (used by the web PWA).

pub mod auth;
pub mod client;

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub mod wasm;

pub use client::{PendingError, RemoteClient, RpcCallError};
