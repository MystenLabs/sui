// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod cli;
pub mod command;
pub use cli::lib::cache::*;
pub use command::run_cmd;
use once_cell::sync::Lazy;

pub static DEBUG_MODE: Lazy<bool> = Lazy::new(|| std::env::var("DEBUG").is_ok());

const LOCAL_CACHE_DIR: &str = ".suiop";
