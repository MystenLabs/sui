//! This module defines a rudimentary interface for JSON RPC 2.0 clients. The current
//! implementation requires the remote endpoint to send responses in the same order as
//! requests are written (subrequests of a batch request can be returned in any order).

// TODO: this lives here because it supports external resolvers, but it is completely independent
// and should maybe be made into its own crate?

use std::collections::BTreeMap;

pub mod client;
pub mod server;
pub mod types;

pub use client::Endpoint;
