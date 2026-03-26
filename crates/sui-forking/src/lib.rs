// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Forking tool for Sui.

mod service_store;
mod startup;

#[allow(unused)]
pub(crate) static VERSION: &str = env!("CARGO_PKG_VERSION");
