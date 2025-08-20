// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_config::node::ExpensiveSafetyCheckConfig;
use sui_types::{
    digests::TransactionDigest, execution_status::ExecutionFailureStatus,
    transaction::TransactionKind,
};
use thiserror::Error;
use tracing::{error, info};

use crate::{
    replay::{ExecutionSandboxState, LocalExec},
    transaction_provider::{TransactionProvider, TransactionSource},
    types::ReplayEngineError,
};

// Step 1: Get a transaction T from the network
// Step 2: Create the sandbox and verify the TX does not fork locally
// Step 3: Create desired mutations of T in set S
// Step 4: For each mutation in S, replay the transaction with the sandbox state from T
//         and verify no panic or invariant violation

pub struct ReplayFuzzerConfig {
    pub num_mutations_per_base: u64,
    pub mutator: Box<dyn TransactionKindMutator + Send + Sync>,
    pub tx_source: TransactionSource,
    pub fail_over_on_err: bool,
    pub expensive_safety_check_config: ExpensiveSafetyCheckConfig,
}

/// Provides the starting transaction for a fuzz session
pub struct ReplayFuzzer {
    pub local_exec: LocalExec,
    pub sandbox_state: ExecutionSandboxState,
    pub config: ReplayFuzzerConfig,
    pub transaction_provider: TransactionProvider,
}

pub trait TransactionKindMutator {
    fn mutate(&mut self, transaction_kind: &TransactionKind) -> Option<TransactionKind>;

    fn reset(&mut self, mutations_per_base: u64);
}

impl ReplayFuzzer {
    pub async fn new(rpc_url: String, config: ReplayFuzzerConfig) -> Result<Self, anyhow::Error> {
        let local_exec = LocalExec::new_from_fn_url(&rpc_url)
            .await?
            .init_for_execution()
            .await?;

        let mut tx_provider = TransactionProvider::new(&rpc_url, config.tx_source.clone()).await?;

        Self::new_with_local_executor(local_exec, config, &mut tx_provider).await
    }

    pub async fn new_with_local_executor(
        mut local_exec: LocalExec,
        config: ReplayFuzzerConfig,
        transaction_provider: &mut TransactionProvider,
    ) -> Result<Self, anyhow::Error> {
        // Seed with the first transaction
        let base_transaction = transaction_provider.next().await?.unwrap_or_else(|| {
            panic!(
                "No transactions found at source: {:?}",
                transaction_provider.source
            )
        });
        let sandbox_state = local_exec
            .execute_transaction(
                &base_transaction,
                config.expensive_safety_check_config.clone(),
                false,
                None,
                None,
                None,
            )
            .await?;

        Ok(Self {
            local_exec,
            sandbox_state,
            config,
            transaction_provider: transaction_provider.clone(),
        })
    }

    pub async fn re_init(mut self) -> Result<Self, anyhow::Error> {
        let local_executor = self
            .local_exec
            .reset_for_new_execution_with_client()
            .await?;
        self.config
            .mutator
            .reset(self.config.num_mutations_per_base);
        Self::new_with_local_executor(local_executor, self.config, &mut self.transaction_provider)
            .await
    }

    pub async fn execute_tx(
        &mut self,
        transaction_kind: &TransactionKind,
    ) -> Result<ExecutionSandboxState, ReplayEngineError> {
        self.local_exec
            .execution_engine_execute_with_tx_info_impl(
                &self.sandbox_state.transaction_info,
                Some(transaction_kind.clone()),
                ExpensiveSafetyCheckConfig::new_enable_all(),
            )
            .await
    }

    pub async fn execute_tx_and_check_status(
        &mut self,
        transaction_kind: &TransactionKind,
    ) -> Result<ExecutionSandboxState, ReplayFuzzError> {
        let sandbox_state = self.execute_tx(transaction_kind).await?;
        if let Some(Err(e)) = &sandbox_state.local_exec_status {
            let stat = e.to_execution_status().0;
            match &stat {
                ExecutionFailureStatus::InvariantViolation
                | ExecutionFailureStatus::VMInvariantViolation => {
                    return Err(ReplayFuzzError::InvariantViolation {
                        tx_digest: sandbox_state.transaction_info.tx_digest,
                        kind: transaction_kind.clone(),
                        exec_status: stat,
                    });
                }
                _ => (),
            }
        }
        Ok(sandbox_state)
    }

    // Simple command and arg shuffle mutation
    // TODO: do more complicated mutations
    pub fn next_mutation(&mut self, transaction_kind: &TransactionKind) -> Option<TransactionKind> {
        self.config.mutator.mutate(transaction_kind)
    }

    pub async fn run(mut self, mut num_base_tx: u64) -> Result<(), ReplayFuzzError> {
        while num_base_tx > 0 {
            let mut tx_kind = self.sandbox_state.transaction_info.kind.clone();

            info!(
                "Starting fuzz with new base TX {}, with at most {} mutations",
                self.sandbox_state.transaction_info.tx_digest, self.config.num_mutations_per_base
            );
            while let Some(mutation) = self.next_mutation(&tx_kind) {
                info!(
                    "Executing mutation: base tx {}, mutation {:?}",
                    self.sandbox_state.transaction_info.tx_digest, mutation
                );
                match self.execute_tx_and_check_status(&mutation).await {
                    Ok(v) => tx_kind = v.transaction_info.kind.clone(),
                    Err(e) => {
                        error!(
                            "Error executing transaction: base tx: {}, mutation: {:?} with error{:?}",
                            self.sandbox_state.transaction_info.tx_digest,
                            mutation, e
                        );
                        if self.config.fail_over_on_err {
                            return Err(e);
                        }
                    }
                }
            }
            info!(
                "Ended fuzz with for base TX {}\n",
                self.sandbox_state.transaction_info.tx_digest
            );
            self = self
                .re_init()
                .await
                .map_err(ReplayEngineError::from)
                .map_err(ReplayFuzzError::from)?;
            num_base_tx -= 1;
        }

        Ok(())
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Error, Clone)]
pub enum ReplayFuzzError {
    #[error(
        "InvariantViolation: digest: {tx_digest}, kind: {kind}, status: {:?}",
        exec_status
    )]
    InvariantViolation {
        tx_digest: TransactionDigest,
        kind: TransactionKind,
        exec_status: ExecutionFailureStatus,
    },

    #[error(
        "LocalExecError: exec system error which may/not be related to fuzzing: {:?}.",
        err
    )]
    LocalExecError { err: ReplayEngineError },
    // TODO: how exactly do we catch this?
    //Panic(TransactionDigest, TransactionKind),
}

impl From<ReplayEngineError> for ReplayFuzzError {
    fn from(err: ReplayEngineError) -> Self {
        ReplayFuzzError::LocalExecError { err }
    }
}
