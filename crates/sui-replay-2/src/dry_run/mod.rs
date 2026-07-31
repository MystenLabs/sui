// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Building blocks for executing uncommitted transactions locally.
//!
//! Local dry-run preparation is separate from historical replay preparation:
//! replay consumes trusted on-chain transaction data, while a dry-run must first
//! validate transaction data and its inputs.

mod artifacts;
mod input_loader;
mod preparation;
mod runtime_store;

use crate::dry_run::runtime_store::CheckpointRuntimeStore;
use crate::execution::{
    CheckedExecutionInputs, ReplayExecutor, TransactionExecutionResult,
    execute_checked_transaction, extend_object_cache_with_created_outputs,
};
use anyhow::Result;
use move_trace_format::format::MoveTraceBuilder;
use std::collections::{BTreeMap, HashSet};
use sui_data_store::{CheckpointExecutionContext, CheckpointObjectStore};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    digests::TransactionDigest,
    effects::TransactionEffects,
    error::ExecutionError,
    execution_params::{ExecutionOrEarlyError, FundsWithdrawStatus, get_early_execution_error},
    gas::SuiGasStatus,
    inner_temporary_store::InnerTemporaryStore,
    object::Object,
    transaction::TransactionData,
};

pub use artifacts::save_local_dry_run_artifacts;
pub use input_loader::{
    PreparedLocalDryRun, prepare_checked_local_dry_run, protocol_config_for_snapshot,
};
pub use preparation::{
    PreparedLocalSimulationTransaction, prepare_transaction_for_local_simulation,
};

// Execution engines below this version accept a trace builder but never write to it.
const MIN_TRACED_EXECUTION_VERSION: u64 = 3;

/// Results from one protocol-checked execution against a finalized checkpoint.
pub struct LocalDryRunExecution {
    /// Coherent chain, checkpoint, and epoch metadata used by the execution.
    pub snapshot: CheckpointExecutionContext,
    /// Transaction after mock-gas normalization and full validity checks.
    pub transaction: TransactionData,
    /// Digest of the normalized transaction.
    pub digest: TransactionDigest,
    /// VM result. VM failures still include effects and an optional trace.
    pub transaction_result: std::result::Result<(), ExecutionError>,
    /// Effects produced by the local execution.
    pub effects: TransactionEffects,
    /// Final gas status produced by the execution engine.
    pub gas_status: SuiGasStatus,
    /// Artifact cache of checkpoint, synthetic local, and created output bodies.
    pub object_cache: BTreeMap<ObjectID, BTreeMap<u64, Object>>,
    /// Temporary writes and events produced by the execution engine.
    pub inner_store: InnerTemporaryStore,
    /// Trace builder populated by the same execution, when requested.
    pub trace_builder: Option<MoveTraceBuilder>,
    /// Synthetic gas object ID, when mock gas was injected.
    pub mock_gas_id: Option<ObjectID>,
}

/// Execute checked inputs against checkpoint state, with injected mock gas kept local.
pub fn execute_prepared_local_dry_run(
    prepared: PreparedLocalDryRun,
    store: &dyn CheckpointObjectStore,
    collect_trace: bool,
) -> Result<LocalDryRunExecution> {
    #[cfg(not(feature = "tracing"))]
    if collect_trace {
        anyhow::bail!(
            "tracing is not enabled in this build; rebuild sui-replay-2 with `--features tracing`"
        );
    }

    let PreparedLocalDryRun {
        snapshot,
        protocol_config,
        transaction,
        digest,
        checked_inputs,
        gas_status,
        checkpoint_objects,
        mock_gas_object,
    } = prepared;
    let mock_gas_id = mock_gas_object.as_ref().map(|object| object.id());

    let execution_version = protocol_config.execution_version_as_option().unwrap_or(0);
    if collect_trace && execution_version < MIN_TRACED_EXECUTION_VERSION {
        return Err(input_loader::unsupported_error(format!(
            "local checkpoint dry-run cannot trace protocol version {} (execution version {})",
            protocol_config.version.as_u64(),
            execution_version,
        )));
    }

    let executor = ReplayExecutor::new(protocol_config)?;
    let certificate_deny_set = HashSet::new();
    let execution_params = match get_early_execution_error(
        &digest,
        &checked_inputs,
        &certificate_deny_set,
        &FundsWithdrawStatus::MaybeSufficient,
    ) {
        None => ExecutionOrEarlyError::ok(None),
        Some(errors) => ExecutionOrEarlyError::failed(errors, None),
    };
    let execution_store = CheckpointRuntimeStore::new(
        &snapshot,
        store,
        checkpoint_objects,
        mock_gas_object.into_iter().collect(),
    );
    let mut trace_builder = collect_trace.then(MoveTraceBuilder::new);
    let TransactionExecutionResult {
        transaction_result,
        effects,
        gas_status,
        inner_store,
    } = execute_checked_transaction(
        &executor,
        &execution_store,
        &transaction,
        digest,
        snapshot.epoch.epoch_id,
        snapshot.epoch.start_timestamp,
        CheckedExecutionInputs {
            input_objects: checked_inputs,
            gas_status,
            execution_params,
            // Fullnode simulation has no consensus-assigned system-object versions.
            system_object_versions: BTreeMap::<ObjectID, SequenceNumber>::new(),
        },
        &mut trace_builder,
    );

    if let Some(error) = execution_store.take_deferred_error() {
        anyhow::bail!("runtime state read failed: {error}");
    }
    if transaction_result.is_ok() && !inner_store.accumulator_running_max_withdraws.is_empty() {
        return Err(input_loader::unsupported_error(
            "local checkpoint dry-run cannot verify accumulator funds used during execution",
        ));
    }

    let mut object_cache = execution_store.into_object_cache();
    extend_object_cache_with_created_outputs(&mut object_cache, &effects, &inner_store)?;

    Ok(LocalDryRunExecution {
        snapshot,
        transaction,
        digest,
        transaction_result,
        effects,
        gas_status,
        object_cache,
        inner_store,
        trace_builder,
        mock_gas_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_data_store::EpochData;
    use sui_protocol_config::ProtocolConfig;
    use sui_types::{
        base_types::SuiAddress,
        digests::{ChainIdentifier, CheckpointDigest},
        effects::TransactionEffectsAPI,
        transaction::{Argument, CallArg, Command, ProgrammableTransaction},
    };

    struct FrameworkCheckpointStore {
        context: CheckpointExecutionContext,
        packages: BTreeMap<ObjectID, Object>,
    }

    impl CheckpointObjectStore for FrameworkCheckpointStore {
        fn get_checkpoint_objects(
            &self,
            context: &CheckpointExecutionContext,
            requests: &[sui_data_store::CheckpointObjectRequest],
        ) -> Result<Vec<Option<Object>>> {
            assert_eq!(context, &self.context);
            Ok(requests
                .iter()
                .map(|request| {
                    assert_eq!(
                        request.selector,
                        sui_data_store::CheckpointObjectSelector::Latest,
                    );
                    self.packages.get(&request.object_id).cloned()
                })
                .collect())
        }
    }

    fn framework_store_for_protocol(protocol_version: u64) -> FrameworkCheckpointStore {
        FrameworkCheckpointStore {
            context: CheckpointExecutionContext {
                chain_identifier: ChainIdentifier::default(),
                checkpoint: 17,
                checkpoint_digest: CheckpointDigest::random(),
                epoch: EpochData {
                    epoch_id: 3,
                    protocol_version,
                    rgp: 1_000,
                    start_timestamp: 123,
                },
            },
            packages: sui_framework::BuiltInFramework::genesis_objects()
                .map(|object| (object.id(), object))
                .collect(),
        }
    }

    fn framework_store() -> FrameworkCheckpointStore {
        framework_store_for_protocol(
            ProtocolConfig::get_for_max_version_UNSAFE()
                .version
                .as_u64(),
        )
    }

    fn mock_gas_transaction(sender: SuiAddress, gas_budget: u64) -> TransactionData {
        let recipient = SuiAddress::random_for_testing_only();
        let programmable = ProgrammableTransaction {
            inputs: vec![CallArg::Pure(bcs::to_bytes(&recipient).unwrap())],
            commands: vec![Command::TransferObjects(
                vec![Argument::GasCoin],
                Argument::Input(0),
            )],
        };
        TransactionData::new_programmable(sender, vec![], programmable, gas_budget, 1_000)
    }

    fn prepared_mock_gas(store: &FrameworkCheckpointStore) -> PreparedLocalDryRun {
        let protocol_config = protocol_config_for_snapshot(&store.context).unwrap();
        prepare_checked_local_dry_run(
            mock_gas_transaction(
                SuiAddress::random_for_testing_only(),
                protocol_config.max_tx_gas(),
            ),
            store.context.clone(),
            store,
            true,
        )
        .unwrap()
    }

    #[test]
    fn executes_checked_transaction_with_mock_gas_through_checkpoint_store() {
        let store = framework_store();
        let result =
            execute_prepared_local_dry_run(prepared_mock_gas(&store), &store, false).unwrap();

        assert!(result.transaction_result.is_ok());
        assert!(result.effects.status().is_ok());
        assert_eq!(result.mock_gas_id, Some(ObjectID::MAX));
        assert!(result.object_cache.contains_key(&ObjectID::MAX));
    }

    #[cfg(feature = "tracing")]
    #[test]
    fn traced_execution_returns_the_same_checked_result() {
        let store = framework_store();
        let result =
            execute_prepared_local_dry_run(prepared_mock_gas(&store), &store, true).unwrap();

        assert!(result.transaction_result.is_ok());
        assert!(result.effects.status().is_ok());
        assert!(result.trace_builder.is_some());
    }

    #[cfg(feature = "tracing")]
    #[test]
    fn tracing_rejects_execution_versions_that_ignore_the_trace_builder() {
        let store = framework_store_for_protocol(37);
        let error = execute_prepared_local_dry_run(prepared_mock_gas(&store), &store, true)
            .err()
            .unwrap();

        assert!(error.to_string().contains("execution version 2"));
    }

    #[cfg(not(feature = "tracing"))]
    #[test]
    fn tracing_request_fails_before_execution_when_feature_is_disabled() {
        let store = framework_store();
        let error = execute_prepared_local_dry_run(prepared_mock_gas(&store), &store, true)
            .err()
            .unwrap();

        assert!(error.to_string().contains("tracing is not enabled"));
    }
}
