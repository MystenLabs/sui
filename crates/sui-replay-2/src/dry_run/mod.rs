// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Building blocks for executing uncommitted transactions locally.
//!
//! Local dry-run preparation is separate from historical replay preparation:
//! replay consumes trusted on-chain transaction data, while a dry-run must first
//! validate transaction data and its inputs.

mod preparation;

pub use preparation::{
    PreparedLocalSimulationTransaction, prepare_transaction_for_local_simulation,
};
