// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `ForkedExecutor` — runs Move modules against real Sui network state fetched
//! lazily from an RPC node, using `dev_inspect_transaction` with
//! `skip_all_checks = true` so that gas, signatures, and transaction validation
//! are all bypassed.
//!
//! Typical usage:
//! ```ignore
//! let mut exec = ForkedExecutor::new("https://fullnode.mainnet.sui.io:443")?;
//! let result = exec.publish_and_call(module_bytes, dep_ids, &[("my_mod", "entry_fn")])?;
//! ```

use std::sync::Arc;

use anyhow::Result;
use move_core_types::identifier::Identifier;
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::error::ExecutionError;
use sui_types::execution_status::ExecutionStatus;
use sui_types::execution::ExecutionResult;
use sui_types::execution_params::ExecutionOrEarlyError;
use sui_types::gas::SuiGasStatus;
use sui_types::inner_temporary_store::InnerTemporaryStore;
use sui_types::metrics::LimitsMetrics;
use sui_types::object::Object;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{
    CheckedInputObjects, GasData, InputObjectKind, InputObjects, ObjectReadResult,
    TransactionKind,
};

use crate::forked_store::ForkedStore;

/// Budget and price constants for dev-inspect execution — gas accounting is
/// bypassed by `skip_all_checks`, but non-zero values are required to pass
/// basic struct validation.
const DEV_INSPECT_GAS_BUDGET: u64 = 50_000_000_000;
const DEV_INSPECT_GAS_PRICE: u64 = 1_000;
const DEV_INSPECT_GAS_BALANCE: u64 = 100_000_000_000;

/// Outcome of a `publish_and_call` invocation.
pub struct ForkedExecResult {
    /// Final transaction effects (created/mutated/deleted objects, gas, status).
    pub effects: TransactionEffects,
    /// Per-command execution results from dev-inspect, or an execution error.
    pub execution_result: Result<Vec<ExecutionResult>, ExecutionError>,
}

pub struct ForkedExecutor {
    store: ForkedStore,
    executor: Arc<dyn sui_execution::Executor + Send + Sync>,
    protocol_config: ProtocolConfig,
    metrics: Arc<LimitsMetrics>,
}

impl ForkedExecutor {
    /// Connect to `rpc_url` and build an executor for `chain` (Mainnet or Testnet).
    pub fn new(rpc_url: &str, chain: Chain) -> Result<Self> {
        let store = ForkedStore::new(rpc_url)?;
        let protocol_config =
            ProtocolConfig::get_for_version(ProtocolVersion::MAX, chain);
        let executor = sui_execution::executor(&protocol_config, /*silent=*/ true)
            .map_err(|e| anyhow::anyhow!("executor creation failed: {e}"))?;
        let metrics = Arc::new(LimitsMetrics::new(&prometheus::Registry::new()));
        Ok(Self {
            store,
            executor,
            protocol_config,
            metrics,
        })
    }

    /// Access the underlying `ForkedStore` to inject oracle overrides, etc.
    pub fn store_mut(&mut self) -> &mut ForkedStore {
        &mut self.store
    }

    /// Publish `module_bytes` with the given framework dependencies, then call
    /// each `(module_name, function_name)` entry function on the published package.
    ///
    /// Steps:
    ///   1. Run a Publish PTB via `dev_inspect_transaction`.
    ///   2. Extract the package ID from effects and inject all written objects
    ///      (the new package) into the override layer.
    ///   3. For each function call, run a separate MoveCall PTB.
    ///
    /// All execution uses `skip_all_checks = true`, bypassing gas validation,
    /// signature checks, and protocol-level limits.
    pub fn publish_and_call(
        &mut self,
        module_bytes: Vec<u8>,
        dep_ids: Vec<ObjectID>,
        function_calls: &[(&str, &str)],
    ) -> Result<ForkedExecResult> {
        // ---- Step 1: Publish --------------------------------------------
        let (publish_inner, publish_effects, publish_result) =
            self.run_publish(module_bytes, dep_ids)?;

        // Persist the written objects (the new package) into the override
        // layer so subsequent MoveCall PTBs can find them.
        for (_, obj) in publish_inner.written {
            self.store.inject_object(obj);
        }

        // If publish failed or there are no function calls, return now.
        if function_calls.is_empty()
            || matches!(publish_effects.status(), ExecutionStatus::Failure { .. })
        {
            return Ok(ForkedExecResult {
                effects: publish_effects,
                execution_result: publish_result,
            });
        }

        // ---- Step 2: Find the package ID --------------------------------
        let package_id = publish_effects
            .created()
            .iter()
            .find(|(_, owner)| owner.is_immutable())
            .map(|(obj_ref, _)| obj_ref.0)
            .ok_or_else(|| anyhow::anyhow!("publish succeeded but no immutable package in effects"))?;

        // ---- Step 3: Call each function ---------------------------------
        let mut last_effects = publish_effects;
        let mut last_result: Result<Vec<ExecutionResult>, ExecutionError> = publish_result;

        for (module_name, fn_name) in function_calls {
            let module_id =
                Identifier::new(*module_name).map_err(|e| anyhow::anyhow!("{e}"))?;
            let fn_id =
                Identifier::new(*fn_name).map_err(|e| anyhow::anyhow!("{e}"))?;

            let (_, effects, result) =
                self.run_move_call(package_id, module_id, fn_id)?;
            last_effects = effects;
            last_result = result;
        }

        Ok(ForkedExecResult {
            effects: last_effects,
            execution_result: last_result,
        })
    }

    // -------------------------------------------------------------------------
    // Internal helpers
    // -------------------------------------------------------------------------

    /// Run a Publish PTB and return `(inner_store, effects, exec_result)`.
    fn run_publish(
        &mut self,
        module_bytes: Vec<u8>,
        dep_ids: Vec<ObjectID>,
    ) -> Result<(InnerTemporaryStore, TransactionEffects, std::result::Result<Vec<ExecutionResult>, ExecutionError>)>
    {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.publish_immutable(vec![module_bytes], dep_ids);
        let pt = builder.finish();
        self.run_pt(TransactionKind::ProgrammableTransaction(pt))
    }

    /// Run a MoveCall PTB calling `package::module::function()` with no args.
    fn run_move_call(
        &mut self,
        package: ObjectID,
        module: Identifier,
        function: Identifier,
    ) -> Result<(InnerTemporaryStore, TransactionEffects, std::result::Result<Vec<ExecutionResult>, ExecutionError>)>
    {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.command(sui_types::transaction::Command::move_call(
            package, module, function, vec![], vec![],
        ));
        let pt = builder.finish();
        self.run_pt(TransactionKind::ProgrammableTransaction(pt))
    }

    /// Core: execute any `TransactionKind` via `dev_inspect_transaction` with a
    /// dummy unmetered gas coin and no signature/protocol checks.
    fn run_pt(
        &mut self,
        kind: TransactionKind,
    ) -> Result<(InnerTemporaryStore, TransactionEffects, std::result::Result<Vec<ExecutionResult>, ExecutionError>)>
    {
        let sender = SuiAddress::ZERO;

        // Create a dummy gas coin and inject it so the executor can find it.
        let dummy_gas =
            Object::new_gas_with_balance_and_owner_for_testing(DEV_INSPECT_GAS_BALANCE, sender);
        let gas_ref = dummy_gas.compute_object_reference();
        self.store.inject_object(dummy_gas.clone());

        // Minimal CheckedInputObjects: just the gas coin.  With skip_all_checks
        // the executor doesn't verify the object set, it just uses the BackingStore.
        let input_objects = InputObjects::new(vec![ObjectReadResult::new(
            InputObjectKind::ImmOrOwnedMoveObject(gas_ref),
            dummy_gas.into(),
        )]);
        let checked = CheckedInputObjects::new_for_replay(input_objects);

        let gas_data = GasData {
            payment: vec![gas_ref],
            owner: sender,
            price: DEV_INSPECT_GAS_PRICE,
            budget: DEV_INSPECT_GAS_BUDGET,
        };

        // new_unmetered() creates a SuiGasStatus with charge=false — gas
        // accounting is fully disabled, matching skip_all_checks semantics.
        let gas_status = SuiGasStatus::new_unmetered();

        // epoch 0 / timestamp 0 is fine for dev-inspect (the executor doesn't
        // enforce epoch validity when skip_all_checks=true).
        let epoch_id: sui_types::committee::EpochId = 0;
        let epoch_ts: u64 = 0;

        let execution_params: ExecutionOrEarlyError = Ok(());
        let tx_digest = TransactionDigest::random();

        let (inner_store, _gas_status, effects, result) =
            self.executor.dev_inspect_transaction(
                &self.store,
                &self.protocol_config,
                self.metrics.clone(),
                false, // enable_expensive_checks
                execution_params,
                &epoch_id,
                epoch_ts,
                checked,
                gas_data,
                gas_status,
                kind,
                sender,
                tx_digest,
                true, // skip_all_checks
            );

        Ok((inner_store, effects, result))
    }
}

#[cfg(test)]
mod tests {
    // Integration tests require a live RPC node.  Run with:
    //   SUI_RPC_URL=https://fullnode.mainnet.sui.io:443 cargo test --features fork -- --ignored

    #[test]
    #[ignore]
    fn test_connect_mainnet() {
        let rpc_url = std::env::var("SUI_RPC_URL")
            .unwrap_or_else(|_| "https://fullnode.mainnet.sui.io:443".to_string());
        let exec =
            super::ForkedExecutor::new(&rpc_url, sui_protocol_config::Chain::Mainnet).unwrap();
        // Just verify construction succeeds.
        drop(exec);
    }
}
