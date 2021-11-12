// Copyright(C) Facebook, Inc. and its affiliates.
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

mod batch_maker;
mod helper;
mod primary_connector;
mod processor;
mod quorum_waiter;
mod synchronizer;
mod worker;

#[cfg(test)]
#[path = "tests/common.rs"]
mod common;

pub use crate::worker::Worker;
