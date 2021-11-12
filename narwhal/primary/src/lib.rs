// Copyright(C) Facebook, Inc. and its affiliates.
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

#[macro_use]
mod error;
mod aggregators;
mod certificate_waiter;
mod core;
mod garbage_collector;
mod header_waiter;
mod helper;
mod messages;
mod payload_receiver;
mod primary;
mod proposer;
mod synchronizer;

#[cfg(test)]
#[path = "tests/common.rs"]
mod common;

pub use crate::messages::{Certificate, Header};
pub use crate::primary::{Primary, PrimaryWorkerMessage, Round, WorkerPrimaryMessage};
