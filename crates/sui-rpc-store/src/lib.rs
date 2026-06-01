// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Storage backend for `sui-rpc-api`.
//!
//! Built on top of [`sui_consistent_store`], this crate hosts the
//! column families that back every read the RPC service performs:
//!
//! - Raw chain data — objects, transactions, effects, events,
//!   checkpoints, committees — previously served by the validator's
//!   perpetual / checkpoint / committee stores.
//! - Indexes — owner, dynamic-field, coin, balance, package version,
//!   epoch info, ledger history — previously served by
//!   `sui-core::rpc_index` and `sui-indexer-alt-consistent-store`.
//!
//! Values are encoded with bespoke protobuf messages defined under
//! `proto/sui/rpc_store/`, mirroring the build setup in
//! `sui-consistent-store`.

pub mod keys;
pub mod proto;
pub mod schema;

pub use crate::schema::RpcStoreSchema;

//TODO
// we may want to introduce a way for some pipelines to be able to be implemented using a
// Concurrent pipeline but we would still want the synchronizer to ensure all pipelines are at
// least up to a certain point before taking a snapshot (concurrent pipelines would be able to run
// ahead but sequential ones would not) The Synchronizer change you flagged (concurrent pipelines
// that can run ahead while sequential ones gate snapshots) is a sui-consistent-store change, not
// this crate's, so I left it out.
