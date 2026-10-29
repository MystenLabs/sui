// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Best-effort Move tracing for transactions already simulated by a fullnode.
//!
//! The fullnode simulation remains the authoritative result. This module reconstructs any
//! synthetic gas used by that simulation and feeds the simulated transaction and effects through
//! the historical replay engine with tracing enabled. A plain GraphQL replay store supplies object
//! bodies, package dependencies, and checkpoint and epoch metadata without using replay's
//! persistent cache.
//!
//! The latest GraphQL checkpoint is selected independently from the fullnode simulation, so the
//! local execution can observe different package or dynamic-field state. Effects divergence is
//! reported as a warning rather than presenting the local replay as authoritative.

use crate::{
    artifacts::ArtifactManager,
    replay_txn::{ExecutorProvider, replay_transaction_from_store},
};
use anyhow::{Context, Error, Result, ensure};
use std::path::{Path, PathBuf};
use sui_data_store::{
    EpochData, EpochStore, Node, ObjectKey, ObjectStore, TransactionInfo, TransactionStore,
    VersionQuery, stores::DataStore,
};
use sui_types::{
    base_types::ObjectID,
    coin_reservation::ParsedDigest,
    digests::TransactionDigest,
    effects::{TransactionEffects, TransactionEffectsAPI},
    object::{MoveObject, OBJECT_START_VERSION, Object, Owner},
    supported_protocol_versions::ProtocolConfig,
    transaction::{TransactionData, TransactionDataAPI},
};

// Duplicated to keep `sui-core` out of the replay crate's dependency graph. Keep this in sync with
// `DEV_INSPECT_GAS_COIN_VALUE` in `sui-core/src/authority.rs`.
const SIMULATION_GAS_COIN_VALUE: u64 = 1_000_000_000_000_000_000;

/// Provenance and artifact status for a locally traced fullnode simulation.
pub struct SimulatedTransactionTrace {
    /// Digest of the transaction the fullnode actually simulated, after mock-gas reconstruction.
    pub digest: TransactionDigest,
    /// Directory containing the replay artifacts for this digest.
    pub artifact_path: PathBuf,
    /// Independently selected GraphQL checkpoint used to bound state reads.
    pub read_checkpoint: u64,
    /// Whether local and fullnode effects matched, or `None` when no local execution occurred.
    pub effects_match: Option<bool>,
    /// Whether `trace.json.zst` was successfully serialized for the local replay.
    pub trace_generated: bool,
    /// Synthetic gas object inserted locally to reproduce the fullnode-executed transaction.
    pub mock_gas_id: Option<ObjectID>,
}

/// Replay data source that serves reconstructed mock gas locally and delegates chain state to
/// GraphQL.
struct SimulationReplayStore {
    /// GraphQL endpoint for checkpoint and epoch metadata, packages, and object bodies.
    source: DataStore,
    /// String form expected by the replay engine's transaction lookup interface.
    transaction_digest: String,
    /// Transaction data and effects returned by the fullnode simulation, adapted for replay.
    transaction: TransactionInfo,
    /// Synthetic mock gas unavailable from GraphQL, when the fullnode used it.
    mock_gas_object: Option<Object>,
}

/// Trace a fullnode simulation and write replay artifacts under `output_root/<digest>`.
///
/// GraphQL supplies object bodies, package dependencies, effects epoch metadata, and replay
/// protocol configuration. Non-package objects named by the transaction or effects are queried at
/// their recorded versions, while dynamic child reads use root-version bounds. Reads without a
/// usable version, such as package and versionless reads, are bounded by GraphQL's latest indexed
/// checkpoint.
///
/// A fullnode simulation is not checkpointed, and its response does not identify the state
/// snapshot used for execution. The fullnode's latest-checkpoint metadata is only a server
/// watermark, so it is not used as the replay anchor. If the simulation API eventually exposes an
/// actual execution-snapshot checkpoint, that could end up being a better anchor.
///
/// Reconstructed mock gas is satisfied locally because it does not exist on-chain.
///
/// `node` must identify the same chain that produced the simulation; this function cannot verify
/// that association from the response alone.
///
/// Existing generated artifacts for this digest are removed before replay. Package source
/// directories and other unrecognized user files are preserved.
///
/// When local effects diverge, `transaction_effects.json` retains the fullnode effects and
/// `forked_transaction_effects.json` records the local result. The caller is expected to warn that
/// the trace came from that different local result.
pub async fn trace_simulated_transaction(
    transaction: TransactionData,
    effects: TransactionEffects,
    node: Node,
    user_agent: &str,
    output_root: &Path,
) -> Result<SimulatedTransactionTrace> {
    // The fullnode can execute a payment-free request with synthetic gas while returning the
    // original transaction data. Rebuild that payment so the digest matches the effects.
    let expected_digest = *effects.transaction_digest();
    let (transaction, mock_gas_object) =
        reconstruct_fullnode_transaction(transaction, expected_digest)?;
    let digest = transaction.digest();
    let network = node.network_name();
    let artifact_path = output_root.join(digest.to_string());

    // This checkpoint bounds only reads for which transaction execution provides no version.
    let source = DataStore::new(node, user_agent)?;
    let read_checkpoint = source.latest_checkpoint_sequence_number().await?;
    let transaction_digest = digest.to_string();
    let mock_gas_id = mock_gas_object.as_ref().map(|object| object.id());

    tokio::task::spawn_blocking(move || {
        let artifact_manager = ArtifactManager::new(&artifact_path, true)?;
        // A rerun must not expose generated artifacts left by an earlier attempt with the same
        // digest.
        artifact_manager.clear_generated_artifacts()?;

        // Keep the uncommitted transaction and mock gas local; on-chain state uses GraphQL.
        let store = SimulationReplayStore {
            source,
            transaction_digest: transaction_digest.clone(),
            transaction: TransactionInfo {
                data: transaction,
                effects,
                checkpoint: read_checkpoint,
            },
            mock_gas_object,
        };

        let mut executor_provider = ExecutorProvider::new(false);
        // Reuse historical replay with tracing enabled and its standard artifact behavior.
        let outcome = replay_transaction_from_store(
            &artifact_manager,
            &transaction_digest,
            &store,
            network,
            true,
            &mut executor_provider,
        )?;

        Ok(SimulatedTransactionTrace {
            digest,
            artifact_path,
            read_checkpoint,
            effects_match: outcome.effects_match,
            trace_generated: outcome.trace_generated,
            mock_gas_id,
        })
    })
    .await
    .context("traced dry-run worker failed")?
}

impl TransactionStore for SimulationReplayStore {
    /// Return the in-memory simulated transaction and expected effects, which cannot be fetched
    /// from GraphQL because the transaction was never committed.
    fn transaction_data_and_effects(
        &self,
        tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, Error> {
        Ok((tx_digest == self.transaction_digest).then(|| self.transaction.clone()))
    }
}

impl EpochStore for SimulationReplayStore {
    /// Fetch epoch metadata from GraphQL because the simulation effects provide only the epoch ID.
    fn epoch_info(&self, epoch: u64) -> Result<Option<EpochData>, Error> {
        self.source.epoch_info(epoch)
    }

    /// Build the protocol configuration from GraphQL-provided epoch metadata and the selected
    /// chain.
    fn protocol_config(&self, epoch: u64) -> Result<Option<ProtocolConfig>, Error> {
        self.source.protocol_config(epoch)
    }
}

impl ObjectStore for SimulationReplayStore {
    /// Retrieve objects by key from reconstructed mock gas and GraphQL state.
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error> {
        let mut results = vec![None; keys.len()];
        // Serve mock gas locally because it does not exist on-chain, and retain each unresolved
        // key's index for the GraphQL lookup.
        let mut source_keys = Vec::new();
        let mut source_indices = Vec::new();

        for (index, key) in keys.iter().enumerate() {
            let local = local_mock_gas(self.mock_gas_object.as_ref(), key);
            if let Some(object) = local {
                results[index] = Some(object);
            } else {
                source_indices.push(index);
                source_keys.push(key.clone());
            }
        }

        let source_results = self.source.get_objects(&source_keys)?;
        assert_eq!(source_indices.len(), source_results.len());
        // Restore GraphQL answers to the positional result required by ObjectStore.
        for (offset, object) in source_results.into_iter().enumerate() {
            results[source_indices[offset]] = object;
        }
        Ok(results)
    }
}

/// Resolve the exact-version lookup used to preload gas payments for replay.
///
/// Root-bounded and checkpoint-qualified reads are not gas-payment lookups and fall through to
/// GraphQL.
fn local_mock_gas(mock_gas_object: Option<&Object>, key: &ObjectKey) -> Option<(Object, u64)> {
    let object = mock_gas_object.filter(|object| object.id() == key.object_id)?;
    let version = object.version().value();

    match key.version_query {
        VersionQuery::Version(requested) if requested == version => Some((object.clone(), version)),
        VersionQuery::Version(_) | VersionQuery::RootVersion(_) | VersionQuery::AtCheckpoint(_) => {
            None
        }
    }
}

/// Reject transaction shapes that simulation replay cannot execute faithfully.
fn ensure_supported_transaction(transaction: &TransactionData) -> Result<()> {
    ensure!(
        !transaction.is_gasless_transaction(),
        "dry-run tracing does not support gasless transactions",
    );
    ensure!(
        !transaction.kind().has_coin_reservations(),
        "dry-run tracing does not support coin reservations in PTB inputs",
    );
    ensure!(
        !transaction
            .gas()
            .iter()
            .any(|object_ref| ParsedDigest::is_coin_reservation_digest(&object_ref.2)),
        "dry-run tracing does not support coin reservations in gas payment",
    );
    Ok(())
}

/// Reconstruct and validate the exact transaction executed by the fullnode.
///
/// For a request without gas payment, the fullnode executes with a deterministic synthetic gas
/// coin while returning the original payment-free transaction data. Insert the same coin locally
/// and require the resulting digest to match the effects; any other mismatch is rejected. The
/// reconstructed transaction must also have a shape supported by replay.
fn reconstruct_fullnode_transaction(
    mut transaction: TransactionData,
    expected_digest: TransactionDigest,
) -> Result<(TransactionData, Option<Object>)> {
    // A matching digest proves that the fullnode executed the returned transaction unchanged;
    // only a mismatch can indicate the need for mock-gas insertion.
    let mock_gas = if transaction.digest() == expected_digest {
        None
    } else {
        ensure!(
            transaction.gas().is_empty(),
            "fullnode effects digest does not match the returned transaction with explicit gas payment",
        );
        let mock_gas = new_mock_gas_object(transaction.gas_owner());
        transaction.gas_data_mut().payment = vec![mock_gas.compute_object_reference()];
        ensure!(
            transaction.digest() == expected_digest,
            "could not reconstruct the transaction executed by fullnode simulation",
        );
        Some(mock_gas)
    };

    ensure_supported_transaction(&transaction)?;
    Ok((transaction, mock_gas))
}

/// Construct the synthetic gas coin used by fullnode simulation.
fn new_mock_gas_object(owner: sui_types::base_types::SuiAddress) -> Object {
    Object::new_move(
        MoveObject::new_gas_coin(
            OBJECT_START_VERSION,
            ObjectID::MAX,
            SIMULATION_GAS_COIN_VALUE,
        ),
        Owner::AddressOwner(owner),
        TransactionDigest::genesis_marker(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_types::{
        base_types::{SequenceNumber, SuiAddress},
        coin_reservation::ParsedObjectRefWithdrawal,
        digests::{ChainIdentifier, ObjectDigest},
        transaction::{CallArg, ObjectArg, ProgrammableTransaction},
    };

    fn transaction(gas: Vec<(ObjectID, SequenceNumber, ObjectDigest)>) -> TransactionData {
        transaction_with_inputs(gas, vec![])
    }

    fn transaction_with_inputs(
        gas: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
        inputs: Vec<CallArg>,
    ) -> TransactionData {
        TransactionData::new_programmable(
            SuiAddress::random_for_testing_only(),
            gas,
            ProgrammableTransaction {
                inputs,
                commands: vec![],
            },
            1_000,
            1,
        )
    }

    /// A payment-free response whose effects include deterministic mock gas must reconstruct the
    /// same gas object and exact executed transaction.
    #[test]
    fn reconstructs_fullnode_mock_gas() {
        let transaction = transaction(vec![]);
        let mut expected = transaction.clone();
        let mock_gas = new_mock_gas_object(expected.gas_owner());
        expected.gas_data_mut().payment = vec![mock_gas.compute_object_reference()];

        let (reconstructed, reconstructed_mock) =
            reconstruct_fullnode_transaction(transaction, expected.digest()).unwrap();

        assert_eq!(reconstructed, expected);
        assert_eq!(reconstructed_mock.unwrap(), mock_gas);
    }

    /// A matching response/effects digest proves the fullnode used the transaction unchanged, so
    /// no mock gas may be added.
    #[test]
    fn preserves_transaction_when_digest_already_matches() {
        let gas = (
            ObjectID::random(),
            SequenceNumber::from_u64(7),
            ObjectDigest::random(),
        );
        let transaction = transaction(vec![gas]);

        let (reconstructed, mock_gas) =
            reconstruct_fullnode_transaction(transaction.clone(), transaction.digest()).unwrap();

        assert_eq!(reconstructed, transaction);
        assert!(mock_gas.is_none());
    }

    /// Gasless execution needs gas-status handling unavailable to replay and must be rejected before
    /// execution.
    #[test]
    fn rejects_gasless_transactions() {
        let mut transaction = transaction(vec![]);
        transaction.gas_data_mut().budget = 0;
        transaction.gas_data_mut().price = 0;
        let expected_digest = transaction.digest();

        let error = reconstruct_fullnode_transaction(transaction, expected_digest).unwrap_err();

        assert!(error.to_string().contains("does not support gasless"));
    }

    /// Reservation-backed PTB inputs require the fullnode's compatibility rewrite and must be
    /// rejected before replay.
    #[test]
    fn rejects_coin_reservations_in_ptb_inputs() {
        let reservation = ParsedObjectRefWithdrawal::new(ObjectID::random(), 3, 100)
            .encode(SequenceNumber::from_u64(2), ChainIdentifier::default());
        let transaction = transaction_with_inputs(
            vec![],
            vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(reservation))],
        );
        let expected_digest = transaction.digest();

        let error = reconstruct_fullnode_transaction(transaction, expected_digest).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("coin reservations in PTB inputs")
        );
    }

    /// Reservation-backed gas payments require the fullnode's compatibility rewrite and must be
    /// rejected before replay.
    #[test]
    fn rejects_coin_reservations_in_gas_payment() {
        let reservation = ParsedObjectRefWithdrawal::new(ObjectID::random(), 3, 100)
            .encode(SequenceNumber::from_u64(2), ChainIdentifier::default());
        let transaction = transaction(vec![reservation]);
        let expected_digest = transaction.digest();

        let error = reconstruct_fullnode_transaction(transaction, expected_digest).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("coin reservations in gas payment")
        );
    }

    /// Only the exact ID/version request used for gas preloading may resolve mock gas; every other
    /// query form must fall through to GraphQL.
    #[test]
    fn mock_gas_answers_only_its_exact_version_query() {
        let object = new_mock_gas_object(SuiAddress::random_for_testing_only());
        let version = object.version().value();
        let exact = ObjectKey {
            object_id: object.id(),
            version_query: VersionQuery::Version(version),
        };
        let wrong_version = ObjectKey {
            object_id: object.id(),
            version_query: VersionQuery::Version(version + 1),
        };
        let root_bounded = ObjectKey {
            object_id: object.id(),
            version_query: VersionQuery::RootVersion(version),
        };
        let wrong_id = ObjectKey {
            object_id: ObjectID::random(),
            version_query: VersionQuery::Version(version),
        };
        let checkpoint = ObjectKey {
            object_id: object.id(),
            version_query: VersionQuery::AtCheckpoint(10),
        };

        assert_eq!(
            local_mock_gas(Some(&object), &exact),
            Some((object.clone(), version))
        );
        assert_eq!(local_mock_gas(Some(&object), &wrong_version), None);
        assert_eq!(local_mock_gas(Some(&object), &root_bounded), None);
        assert_eq!(local_mock_gas(Some(&object), &wrong_id), None);
        assert_eq!(local_mock_gas(Some(&object), &checkpoint), None);
        assert_eq!(local_mock_gas(None, &exact), None);
    }
}
