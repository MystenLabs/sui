// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! [`RpcStateReader`] rollup — composes [`ObjectStore`],
//! [`ReadStore`], [`ChildObjectResolver`], and [`RpcIndexes`] (all
//! impl'd in sibling modules) into the top-level trait
//! `sui-rpc-api` consumes.
//!
//! Adds three rpc-api-specific entry points:
//!
//! - [`get_lowest_available_checkpoint_objects`]: returns the
//!   pruning watermark's `tx_seq_lo`-derived checkpoint floor. We
//!   reuse `pruning_watermark.checkpoint_lo` for now — the rpc-api
//!   trait distinguishes "checkpoint data available" from "object
//!   data available" but in this store both axes prune together.
//! - [`get_chain_identifier`]: reads any pipeline's recorded chain
//!   id from the framework's `__chain_id` CF (every pipeline
//!   records the same chain id on first contact).
//! - [`indexes`]: returns `Some(self)` so callers reach the
//!   [`RpcIndexes`] impl through the same handle.
//! - [`get_struct_layout_with_overlay`]: delegates to the
//!   [`resolve_struct_layout`] shim in `layout.rs`.

use sui_consistent_store::FrameworkSchema;
use sui_consistent_store::reader::Reader;
use sui_types::digests::ChainIdentifier;
use sui_types::digests::CheckpointDigest;
use sui_types::full_checkpoint_content::ObjectSet;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::storage::RpcIndexes;
use sui_types::storage::RpcStateReader;
use sui_types::storage::error::Error as StorageError;
use sui_types::storage::error::Result as StorageResult;

use crate::reader::RpcStoreReader;

impl<R: Reader + Send + Sync> RpcStateReader for RpcStoreReader<R> {
    fn get_lowest_available_checkpoint_objects(&self) -> StorageResult<CheckpointSequenceNumber> {
        // Object availability tracks the same pruning watermark
        // axis as checkpoint availability in this store; both
        // CFs are pruned together by `pruning_watermark`.
        let watermarks = self
            .schema()
            .get_pruning_watermarks()
            .map_err(StorageError::custom)?
            .unwrap_or_default();
        Ok(watermarks.checkpoint_lo)
    }

    fn get_chain_identifier(&self) -> StorageResult<ChainIdentifier> {
        // The framework's `__chain_id` CF has one entry per
        // pipeline, every entry agreeing on the same chain id.
        // Take the first row we see.
        let framework = FrameworkSchema::new(self.db().clone());
        let first = framework
            .chain_ids
            .iter(..)
            .map_err(StorageError::custom)?
            .next();
        let Some(entry) = first else {
            return Err(StorageError::missing(
                "no chain id recorded — no pipeline has observed a checkpoint yet",
            ));
        };
        let (_, chain_id) = entry.map_err(StorageError::custom)?;
        Ok(ChainIdentifier::from(CheckpointDigest::new(chain_id.0)))
    }

    fn indexes(&self) -> Option<&dyn RpcIndexes> {
        Some(self)
    }

    fn get_struct_layout_with_overlay(
        &self,
        struct_tag: &move_core_types::language_storage::StructTag,
        overlay: &ObjectSet,
    ) -> StorageResult<Option<move_core_types::annotated_value::MoveTypeLayout>> {
        self.resolve_struct_layout(struct_tag, overlay)
    }
}
