// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Indexer pipelines that populate the `sui-rpc-store` schema
//! from observed [`Checkpoint`]s.
//!
//! Each submodule implements the `Processor` + `sequential::Handler`
//! pair the `sui-indexer-alt-framework` drives: `process` turns a
//! checkpoint into a `Vec<Value>` (with the heavy lifting done in
//! the processor-pool, off the commit hot path), `batch` folds
//! many values into a single `Batch`, and `commit` stages the
//! batch's writes against a [`Connection`] from
//! [`sui_consistent_store::Store`].
//!
//! Every pipeline targets the same backing [`RpcStoreSchema`].

pub mod checkpoint_contents;
pub mod checkpoint_seq_by_digest;
pub mod checkpoint_summary;
pub mod effects;
pub mod events;
pub mod transactions;
pub mod tx_metadata_by_seq;
pub mod tx_seq_by_digest;

use sui_types::full_checkpoint_content::Checkpoint;

use crate::RpcStoreSchema;

/// The schema parameter the framework's `Store` / pipelines bind
/// to.
pub type Schema = RpcStoreSchema;

/// The store type pipelines commit through.
pub type Store = sui_consistent_store::Store<Schema>;

/// The sequence number of the first transaction in `checkpoint`.
///
/// `network_total_transactions` is the cumulative network-wide tx
/// count *after* this checkpoint executes, so subtracting the
/// number of transactions the checkpoint contains gives the
/// `tx_seq` of its first entry.
pub fn first_tx_seq(checkpoint: &Checkpoint) -> u64 {
    checkpoint.summary.network_total_transactions - checkpoint.transactions.len() as u64
}

/// The `tx_seq` of the transaction at index `i` within
/// `checkpoint`.
pub fn tx_seq_at(checkpoint: &Checkpoint, i: usize) -> u64 {
    first_tx_seq(checkpoint) + i as u64
}
