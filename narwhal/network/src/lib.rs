// Copyright(C) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

mod error;
mod receiver;
mod reliable_sender;
mod simple_sender;

#[cfg(test)]
#[path = "tests/common.rs"]
pub mod common;

pub use crate::{
    receiver::{MessageHandler, Receiver, Writer},
    reliable_sender::{CancelHandler, ReliableSender},
    simple_sender::SimpleSender,
};
