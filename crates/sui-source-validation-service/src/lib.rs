// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// actix_web is not supported by mysten-sim, so we don't compile it in simtest
#[cfg(not(msim))]
mod server;

#[cfg(not(msim))]
pub use server::*;
