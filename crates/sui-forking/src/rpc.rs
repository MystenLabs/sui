// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Read-only `ReadStore` and `RpcStateReader` implementations for [`DataStore`],
//! used to serve a `sui-rpc-api` gRPC endpoint on top of a forked network.
//! Unimplemented methods are stubbed with `todo!()` so missing capabilities
//! surface loudly instead of returning empty data.

use std::sync::Arc;

use tracing::info;

use move_core_types::annotated_value::MoveTypeLayout;
use move_core_types::language_storage::StructTag;

use simulacrum::store::SimulatorStore;
use sui_protocol_config::Chain;
use sui_types::committee::Committee;
use sui_types::digests::CheckpointDigest;
use sui_types::digests::TransactionDigest;
use sui_types::digests::get_mainnet_chain_identifier;
use sui_types::digests::get_testnet_chain_identifier;
use sui_types::digests::{ChainIdentifier, CheckpointContentsDigest};
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEvents;
use sui_types::full_checkpoint_content::ObjectSet;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::messages_checkpoint::VersionedFullCheckpointContents;
use sui_types::storage::ObjectKey;
use sui_types::storage::ReadStore;
use sui_types::storage::RpcStateReader;
use sui_types::storage::error::Error as StorageError;
use sui_types::storage::error::Result as StorageResult;
use sui_types::transaction::VerifiedTransaction;

use crate::store::DataStore;

impl ReadStore for DataStore {
    fn get_committee(&self, _epoch: sui_types::committee::EpochId) -> Option<Arc<Committee>> {
        todo!("ReadStore::get_committee on forked DataStore")
    }

    fn get_latest_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
        self.get_highest_verified_checkpoint()
            .map_err(|e| StorageError::custom(e.to_string()))?
            .ok_or_else(|| StorageError::missing("no checkpoint persisted yet"))
    }

    fn get_highest_verified_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
        // As the forked network produces checkpoint, this method will return the latest checkpoint.
        // If no checkpoint has been produced yet, it returns an error indicating that no checkpoint
        // is persisted.
        self.get_highest_verified_checkpoint()
            .map_err(|e| StorageError::custom(e.to_string()))?
            .ok_or_else(|| StorageError::missing("no checkpoint persisted yet"))
    }

    fn get_highest_synced_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
        // A fork has no concept of an "unsynced" checkpoint — anything we hold
        // locally was either pre-fetched at startup or produced by the local
        // executor, so highest-synced collapses to highest-verified.
        self.get_highest_verified_checkpoint()
            .map_err(|e| StorageError::custom(e.to_string()))?
            .ok_or_else(|| {
                StorageError::missing(
                    "no checkpoint persisted yet — cannot determine highest synced checkpoint",
                )
            })
    }

    fn get_lowest_available_checkpoint(&self) -> StorageResult<CheckpointSequenceNumber> {
        Ok(0)
    }

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        self.get_checkpoint_by_digest(digest).ok().flatten()
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        info!("Requested checkpoint {} through gRPC", sequence_number);
        self.get_checkpoint_by_sequence_number(sequence_number)
            .ok()
            .flatten()
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        self.get_checkpoint_contents_by_digest(digest)
            .ok()
            .flatten()
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents> {
        self.get_checkpoint_contents_by_sequence_number(sequence_number)
            .ok()
            .flatten()
    }

    fn get_transaction(&self, tx_digest: &TransactionDigest) -> Option<Arc<VerifiedTransaction>> {
        SimulatorStore::get_transaction(self, tx_digest).map(Arc::new)
    }

    fn get_transaction_effects(&self, tx_digest: &TransactionDigest) -> Option<TransactionEffects> {
        SimulatorStore::get_transaction_effects(self, tx_digest)
    }

    fn get_events(&self, tx_digest: &TransactionDigest) -> Option<TransactionEvents> {
        SimulatorStore::get_transaction_events(self, tx_digest)
    }

    fn get_unchanged_loaded_runtime_objects(
        &self,
        _digest: &TransactionDigest,
    ) -> Option<Vec<ObjectKey>> {
        None
    }

    fn get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> Option<CheckpointSequenceNumber> {
        self.get_transaction_checkpoint(digest).ok().flatten()
    }

    fn get_full_checkpoint_contents(
        &self,
        sequence_number: Option<CheckpointSequenceNumber>,
        digest: &CheckpointContentsDigest,
    ) -> Option<VersionedFullCheckpointContents> {
        todo!()
    }
}

impl RpcStateReader for DataStore {
    fn get_lowest_available_checkpoint_objects(&self) -> StorageResult<CheckpointSequenceNumber> {
        Ok(0)
    }

    fn get_chain_identifier(&self) -> StorageResult<ChainIdentifier> {
        // Map concrete `Chain` enum onto the canonical chain identifier so
        // clients see this fork as the network it's based on. Devnet/custom
        // forks fall back to the forked checkpoint's digest because those
        // chains don't have a stable on-disk identifier.
        let id = match self.chain() {
            Chain::Mainnet => get_mainnet_chain_identifier(),
            Chain::Testnet => get_testnet_chain_identifier(),
            Chain::Unknown => {
                let checkpoint =
                    ReadStore::get_checkpoint_by_sequence_number(self, self.forked_at_checkpoint())
                        .ok_or_else(|| {
                            StorageError::missing(
                                "forked checkpoint missing — cannot derive chain identifier",
                            )
                        })?;
                ChainIdentifier::from(*checkpoint.digest())
            }
        };
        Ok(id)
    }

    fn indexes(&self) -> Option<&dyn sui_types::storage::RpcIndexes> {
        None
    }

    fn get_struct_layout_with_overlay(
        &self,
        _struct_tag: &StructTag,
        _overlay: &ObjectSet,
    ) -> StorageResult<Option<MoveTypeLayout>> {
        Ok(None)
    }
}
