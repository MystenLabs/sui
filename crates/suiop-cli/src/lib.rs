// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod cli;
pub mod command;
pub use command::run_cmd;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref DEBUG_MODE: bool = std::env::var("DEBUG").is_ok();
}
