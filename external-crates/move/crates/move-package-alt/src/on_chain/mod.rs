// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! On-chain dependency fetching and manifest generation.
//!
//! When a package depends on an on-chain package, we need to:
//! 1. Download the bytecode and linkage table from the network (via [`MoveFlavor`])
//! 2. Generate a synthetic `Move.toml` and `Published.toml` in the cache directory
//! 3. Write the bytecode modules to disk
//!
//! The cache layout is `~/.move/on-chain/<chain_id>/<address>/`.

mod errors;
pub(crate) mod fetch;

pub(crate) use errors::{OnChainError, OnChainResult};
