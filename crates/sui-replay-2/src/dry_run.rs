// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Dry-run support: execute a transaction that was never committed on chain against
//! the latest chain state, locally, similarly to how fullnode's `dryRunTransactionBlock`
//! would, and save the same artifacts replay saves (including a Move trace when
//! requested).
//!
//! Where replay reconstructs the historical state a transaction executed against
//! (exact object versions from on-chain effects, packages at the transaction's
//! checkpoint), dry-run anchors all unversioned reads to the latest checkpoint known
//! to the data store: shared objects, packages and dynamic loads resolve to their
//! latest versions as of that checkpoint, mirroring what a fullnode reads from its
//! live state (with the added benefit of a consistent snapshot).
//!
//! The entry point is `dry_run_transaction`, meant to be called with a fully built
//! `TransactionData` (e.g. by `sui client ptb --dry-run --trace`, which builds the
//! transaction from PTB commands the same way it would for execution).

use crate::{
    DEFAULT_OUTPUT_DIR,
    artifacts::{Artifact, ArtifactManager, MoveCallInfo, ReplayCacheSummary},
    execution::{ExecutionContext, execute_to_effects},
    print_effects_or_fork,
    replay_txn::{
        ExecutorProvider, ObjectVersion, get_input_ids, get_packages, load_objects, load_packages,
        resolve_input_objects, verify_txn_and_save_effects,
    },
    tracing::save_trace_output,
};
use anyhow::{Error, Result, anyhow, bail};
use move_trace_format::format::MoveTraceBuilder;
use std::{
    collections::{BTreeMap, btree_map::Entry},
    path::{Path, PathBuf},
};
use sui_data_store::{
    LiveDataStore, Node, ObjectKey, ObjectStore, ReadDataStore, SetupStore, VersionQuery,
    stores::DataStore,
};
use sui_types::{
    base_types::ObjectID,
    digests::TransactionDigest,
    gas::SuiGasStatus,
    object::{MoveObject, OBJECT_START_VERSION, Object, Owner},
    transaction::{CallArg, ObjectArg, TransactionData, TransactionDataAPI, TransactionKind},
};
use tracing::{debug, info_span, warn};

// Balance of the gas coin mocked when a transaction comes with no gas payment.
// Must match `DEV_INSPECT_GAS_COIN_VALUE` in `sui-core/src/authority.rs` (not imported
// to avoid a dependency on `sui-core`) so dry-run behaves like the fullnode's.
const DRY_RUN_GAS_COIN_VALUE: u64 = 1_000_000_000_000_000_000;

/// Options for a dry-run invocation.
#[derive(Clone, Debug)]
pub struct DryRunOptions {
    /// Generate a Move trace of the execution. Requires a binary built with the
    /// `tracing` feature.
    pub trace: bool,
    /// Root directory for the artifacts; they are saved under
    /// `<output_dir>/<transaction digest>/`. Defaults to `<cur_dir>/.replay`.
    pub output_dir: Option<PathBuf>,
    /// Print the computed effects and gas report to stdout.
    pub show_effects: bool,
    /// Overwrite artifacts of a previous run of the same transaction, instead of
    /// raising an error if they already exist.
    pub overwrite: bool,
}

impl Default for DryRunOptions {
    fn default() -> Self {
        Self {
            trace: false,
            output_dir: None,
            show_effects: true,
            overwrite: false,
        }
    }
}

/// Dry-run a transaction against the latest state of the given network. The
/// transaction is executed locally -- nothing is submitted to the chain -- and the
/// same artifacts replay saves are saved for it (including a Move trace when
/// requested), so the Move trace debugger can be used on dry-run transactions too.
/// Returns the directory the artifacts were saved to.
pub fn dry_run_transaction(
    txn_data: TransactionData,
    node: Node,
    user_agent: &str,
    options: DryRunOptions,
) -> Result<PathBuf, Error> {
    // If trying to trace but the binary was not built with the tracing feature flag
    // raise an error (mirrors the check for replay tracing in `handle_replay_config`).
    #[cfg(not(feature = "tracing"))]
    if options.trace {
        bail!(
            "Tracing is not enabled in this build. Please rebuild with the \
            `tracing` feature (`--features tracing`) to use tracing in dry-run"
        );
    }

    let output_root_dir = match &options.output_dir {
        Some(dir) => dir.clone(),
        None => std::env::current_dir()
            .map_err(|e| anyhow!("Failed to get current directory: {e}"))?
            .join(DEFAULT_OUTPUT_DIR),
    };
    let network = node.network_name();
    let data_store = DataStore::new(node, user_agent)
        .map_err(|e| anyhow!("Failed to create data store: {:?}", e))?;
    run_dry_run(&data_store, &output_root_dir, txn_data, network, &options)
}

// Dry-run a single transaction against the latest state served by `data_store` and
// save the artifacts under `<output_root_dir>/<transaction digest>/`.
fn run_dry_run<S>(
    data_store: &S,
    output_root_dir: &Path,
    mut txn_data: TransactionData,
    network: String,
    options: &DryRunOptions,
) -> Result<PathBuf, Error>
where
    S: ReadDataStore + LiveDataStore + SetupStore,
{
    data_store.setup(None)?;

    // Anchor all unversioned reads to the latest checkpoint and use the current
    // epoch's context (protocol config, reference gas price, start timestamp),
    // the same context a fullnode dry-run would execute in.
    let checkpoint = data_store.latest_checkpoint()?;
    let epoch_data = data_store.latest_epoch_info()?;
    let epoch = epoch_data.epoch_id;
    let _span = info_span!("dry_run_tx", epoch, checkpoint).entered();

    let mut executor_provider = ExecutorProvider::new(false);
    let executor = executor_provider.get_or_create(epoch, data_store)?;
    let protocol_config = executor.protocol_config().clone();

    if txn_data.kind().is_system_tx() {
        bail!("dry-run does not support system transactions");
    }
    txn_data
        .kind()
        .validity_check(&protocol_config)
        .map_err(|e| anyhow!("Transaction failed validity checks: {e}"))?;

    // Collect the object queries before mock gas injection so the (nonexistent) mock
    // gas coin is not queried; it is added to the object cache directly after loading.
    let object_keys = get_dry_run_object_keys(&txn_data, checkpoint)?;

    // Mirror fullnode dry-run behavior (`AuthorityState::simulate_transaction`): if the
    // transaction has no gas payment, inject a mock gas coin so gas checks and metering
    // can proceed. Skip for gasless transactions, which do not use gas coins.
    let is_gasless = protocol_config.enable_gasless() && txn_data.is_gasless_transaction();
    let mock_gas_object = if txn_data.gas_data().payment.is_empty() && !is_gasless {
        let object = Object::new_move(
            MoveObject::new_gas_coin(OBJECT_START_VERSION, ObjectID::MAX, DRY_RUN_GAS_COIN_VALUE),
            Owner::AddressOwner(txn_data.gas_data().owner),
            TransactionDigest::genesis_marker(),
        );
        txn_data.gas_data_mut().payment = vec![object.compute_object_reference()];
        Some(object)
    } else {
        None
    };

    // Fail early with a clear error on invalid gas parameters rather than in the
    // execution core.
    SuiGasStatus::new(
        txn_data.gas_data().budget,
        txn_data.gas_data().price,
        epoch_data.rgp,
        &protocol_config,
    )
    .map_err(|e| anyhow!("Invalid gas parameters: {e}"))?;

    // The digest is computed after mock gas injection so it matches the digest in the
    // produced effects (the fullnode dry-run behaves the same way).
    let digest = txn_data.digest();
    let tx_digest = digest.to_string();
    debug!(tx_digest = %tx_digest, "dry-run transaction");

    let tx_dir = output_root_dir.join(&tx_digest);
    let artifact_manager = ArtifactManager::new(&tx_dir, options.overwrite)?;

    // Load all objects and packages used by the transaction.
    let mut object_cache = load_dry_run_objects(&object_keys, &txn_data, checkpoint, data_store)?;
    if let Some(mock_gas_object) = mock_gas_object {
        object_cache
            .entry(mock_gas_object.id())
            .or_default()
            .insert(mock_gas_object.version().value(), mock_gas_object);
    }
    let input_objects = resolve_input_objects(&txn_data, &object_cache, &digest)?;

    // Execute.
    let mut trace_builder_opt = options.trace.then(MoveTraceBuilder::new);
    let (result, context_and_effects) = execute_to_effects(
        ExecutionContext {
            digest,
            txn_data,
            expected_effects: None,
            input_objects,
            executor,
            object_cache,
            epoch_data,
            checkpoint,
        },
        data_store,
        &mut trace_builder_opt,
    )?;

    if let Some(trace_builder) = trace_builder_opt {
        save_trace_output(&artifact_manager, trace_builder, &context_and_effects).map_err(|e| {
            anyhow!(
                "transaction {} failed to build a trace output path -> {:?}",
                tx_digest,
                e
            )
        })?;
    }

    debug!(
        tx_digest = %tx_digest,
        result = ?result,
        output_dir = %artifact_manager.base_path.display(),
        "Dry-ran transaction",
    );

    save_dry_run_artifacts(&artifact_manager, &context_and_effects, network)?;

    println!("Dry-ran transaction {}", tx_digest);
    println!(
        "Artifacts saved to {}",
        artifact_manager.base_path.display()
    );
    print_effects_or_fork(
        &tx_digest,
        output_root_dir,
        options.show_effects,
        &mut std::io::stdout(),
    )?;

    Ok(tx_dir)
}

// Object queries for a dry-run: owned/receiving/gas inputs at the exact versions
// pinned in the transaction data, and shared objects at their latest version as of
// the anchor checkpoint (the dry-run equivalent of the consensus-assigned versions
// replay reads from effects).
fn get_dry_run_object_keys(
    txn_data: &TransactionData,
    checkpoint: u64,
) -> Result<Vec<ObjectKey>, Error> {
    let mut object_keys = get_input_ids(txn_data)?;
    if let TransactionKind::ProgrammableTransaction(ptb) = txn_data.kind() {
        for input in &ptb.inputs {
            if let CallArg::Object(ObjectArg::SharedObject { id, .. }) = input {
                object_keys.insert(ObjectKey {
                    object_id: *id,
                    version_query: VersionQuery::AtCheckpoint(checkpoint),
                });
            }
        }
    }
    Ok(object_keys.into_iter().collect())
}

// Load the objects and packages used by the transaction, all anchored at the given
// checkpoint. Dry-run counterpart of `replay_txn::load_transaction_objects`.
fn load_dry_run_objects(
    object_keys: &[ObjectKey],
    txn_data: &TransactionData,
    checkpoint: u64,
    object_store: &dyn ObjectStore,
) -> Result<BTreeMap<ObjectID, BTreeMap<ObjectVersion, Object>>, Error> {
    // collect all package ids required by the transaction
    let mut packages = get_packages(txn_data)?;

    // load the objects and collect the package ids of the type parameters
    let (mut object_cache, tp_pkgs) = load_objects(object_keys, object_store)?;
    packages.extend(&tp_pkgs);

    // load the packages and add them to the object cache
    let pkg_objects = load_packages(&packages, checkpoint, object_store)?;
    for (object_id, versions) in pkg_objects {
        match object_cache.entry(object_id) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().extend(versions);
            }
            Entry::Vacant(entry) => {
                entry.insert(versions);
            }
        }
    }

    Ok(object_cache)
}

// Save the dry-run artifacts: transaction data, gas report, cache summary, move call
// info and effects. Mirrors the artifact set replay saves, without the effects
// comparison (there are no on-chain effects to compare against).
fn save_dry_run_artifacts(
    artifact_manager: &ArtifactManager<'_>,
    context_and_effects: &crate::execution::TxnContextAndEffects,
    network: String,
) -> Result<(), Error> {
    use sui_types::gas::SuiGasStatusAPI;

    artifact_manager
        .member(Artifact::TransactionData)
        .serialize_artifact(&context_and_effects.txn_data)
        .transpose()?
        .unwrap();

    artifact_manager
        .member(Artifact::TransactionGasReport)
        .serialize_artifact(&context_and_effects.gas_status.gas_usage_report())
        .transpose()?
        .unwrap();

    let cache_summary = ReplayCacheSummary::from_cache(
        context_and_effects.epoch,
        context_and_effects.checkpoint,
        network,
        context_and_effects.protocol_version,
        &context_and_effects.object_cache,
    );
    artifact_manager
        .member(Artifact::ReplayCacheSummary)
        .serialize_artifact(&cache_summary)
        .transpose()?
        .unwrap();

    if let TransactionKind::ProgrammableTransaction(ptb) = context_and_effects.txn_data.kind() {
        match MoveCallInfo::from_transaction(ptb, &context_and_effects.object_cache) {
            Ok(move_call_info) => {
                artifact_manager
                    .member(Artifact::MoveCallInfo)
                    .serialize_artifact(&move_call_info)
                    .transpose()?
                    .unwrap();
            }
            Err(e) => {
                warn!("Failed to extract move call info: {}", e);
            }
        }
    }

    verify_txn_and_save_effects(
        artifact_manager,
        None,
        &context_and_effects.execution_effects,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_types::{
        base_types::{SequenceNumber, SuiAddress},
        digests::ObjectDigest,
        transaction::{GasData, ProgrammableTransaction, SharedObjectMutability},
    };

    #[test]
    fn dry_run_object_keys_resolution() {
        let owned_id = ObjectID::random();
        let shared_id = ObjectID::random();
        let gas_id = ObjectID::random();
        let sender = SuiAddress::ZERO;
        let anchor_checkpoint = 42;

        let kind = TransactionKind::ProgrammableTransaction(ProgrammableTransaction {
            inputs: vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject((
                    owned_id,
                    SequenceNumber::from_u64(3),
                    ObjectDigest::random(),
                ))),
                CallArg::Object(ObjectArg::SharedObject {
                    id: shared_id,
                    initial_shared_version: SequenceNumber::from_u64(1),
                    mutability: SharedObjectMutability::Mutable,
                }),
            ],
            commands: vec![],
        });
        let txn_data = TransactionData::new_with_gas_data(
            kind,
            sender,
            GasData {
                payment: vec![(gas_id, SequenceNumber::from_u64(7), ObjectDigest::random())],
                owner: sender,
                price: 1000,
                budget: 1_000_000,
            },
        );

        let keys = get_dry_run_object_keys(&txn_data, anchor_checkpoint).unwrap();

        // Owned inputs and gas coins are pinned to the versions in the transaction
        // data; shared objects resolve to the latest version at the anchor checkpoint.
        assert!(keys.contains(&ObjectKey {
            object_id: owned_id,
            version_query: VersionQuery::Version(3),
        }));
        assert!(keys.contains(&ObjectKey {
            object_id: gas_id,
            version_query: VersionQuery::Version(7),
        }));
        assert!(keys.contains(&ObjectKey {
            object_id: shared_id,
            version_query: VersionQuery::AtCheckpoint(anchor_checkpoint),
        }));
        assert_eq!(keys.len(), 3);
    }
}
