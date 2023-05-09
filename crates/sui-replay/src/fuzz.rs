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
    data_fetcher::DataFetcher,
    replay::{ExecutionSandboxState, LocalExec},
    types::LocalExecError,
};
use rand::{rngs::ThreadRng, seq::SliceRandom};

// Step 1: Get a transaction T from the network
// Step 2: Create the sandbox and verify the TX does not fork locally
// Step 3: Create desired mutations of T in set S
// Step 4: For each mutation in S, replay the transaction with the sandbox state from T
//         and verify no panic or invariant violation

pub struct ReplayFuzzerConfig {
    pub checkpoint_id_start: Option<u64>,
    pub checkpoint_id_end: Option<u64>,
    pub num_mutations_per_base: u64,

    pub mutator: Box<dyn TransactionKindMutator>,
}

/// Provides the starting transaction for a fuzz session
pub struct ReplayFuzzer {
    pub base_transaction: TransactionDigest,
    pub local_exec: LocalExec,
    pub sandbox_state: ExecutionSandboxState,
    pub config: ReplayFuzzerConfig,
}

pub trait TransactionKindMutator {
    fn mutate(&mut self, transaction_kind: &TransactionKind) -> Option<TransactionKind>;

    fn reset(&mut self, mutations_per_base: u64);
}

impl ReplayFuzzer {
    pub async fn new(
        rpc_url: String,
        base_transaction: Option<TransactionDigest>,
        config: ReplayFuzzerConfig,
    ) -> Result<Self, anyhow::Error> {
        let local_exec = LocalExec::new_from_fn_url(&rpc_url)
            .await?
            .init_for_execution()
            .await?;

        Self::new_with_local_executor(local_exec, base_transaction, config).await
    }

    pub async fn new_with_local_executor(
        mut local_exec: LocalExec,
        base_transaction: Option<TransactionDigest>,
        config: ReplayFuzzerConfig,
    ) -> Result<Self, anyhow::Error> {
        let base_transaction = base_transaction.unwrap_or(
            local_exec
                .fetcher
                .fetch_random_tx(config.checkpoint_id_start, config.checkpoint_id_end)
                .await?,
        );

        let sandbox_state = local_exec
            .execute_transaction(
                &base_transaction,
                ExpensiveSafetyCheckConfig::new_enable_all(),
                false,
            )
            .await?;

        Ok(Self {
            base_transaction,
            local_exec,
            sandbox_state,
            config,
        })
    }

    pub async fn re_init(
        mut self,
        base_transaction: Option<TransactionDigest>,
    ) -> Result<Self, anyhow::Error> {
        let local_executor = self.local_exec.reset_for_new_execution().await?;
        self.config
            .mutator
            .reset(self.config.num_mutations_per_base);
        Self::new_with_local_executor(local_executor, base_transaction, self.config).await
    }

    pub async fn execute_tx(
        &mut self,
        transaction_kind: &TransactionKind,
    ) -> Result<ExecutionSandboxState, LocalExecError> {
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
        if let Err(e) = &sandbox_state.local_exec_status {
            let stat = e.to_execution_status().0;
            match &stat {
                ExecutionFailureStatus::InvariantViolation
                | ExecutionFailureStatus::VMInvariantViolation => {
                    return Err(ReplayFuzzError::InvariantViolation {
                        tx_digest: self.base_transaction,
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
        let mut tx_kind = self.sandbox_state.transaction_info.kind.clone();

        while num_base_tx > 0 {
            info!(
                "Starting fuzz with new base TX {}",
                self.sandbox_state.transaction_info.tx_digest
            );

            while let Some(mutation) = self.next_mutation(&tx_kind) {
                let status = self.execute_tx_and_check_status(&mutation).await;
                if let Err(ReplayFuzzError::InvariantViolation {
                    tx_digest,
                    kind,
                    exec_status,
                }) = &status
                {
                    error!(
                        "Invariant violation: tx digest: {:?}\n kind: {:#?}\nstatus{:?}",
                        tx_digest, kind, exec_status
                    );
                    return Err(status.unwrap_err());
                };
                tx_kind = status.unwrap().transaction_info.kind.clone();
            }
            self = self.re_init(None).await.unwrap();
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
    LocalExecError { err: LocalExecError },
    // TODO: how exactly do we catch this?
    //Panic(TransactionDigest, TransactionKind),
}

impl From<LocalExecError> for ReplayFuzzError {
    fn from(err: LocalExecError) -> Self {
        ReplayFuzzError::LocalExecError { err }
    }
}

pub struct ShuffleMutator {
    pub rng: ThreadRng,
    pub num_mutations_per_base_left: u64,
}

impl TransactionKindMutator for ShuffleMutator {
    fn mutate(&mut self, transaction_kind: &TransactionKind) -> Option<TransactionKind> {
        if self.num_mutations_per_base_left == 0 {
            // Nothing else to do
            return None;
        }

        self.num_mutations_per_base_left -= 1;
        if let TransactionKind::ProgrammableTransaction(mut p) = transaction_kind.clone() {
            // Simple command and arg shuffle mutation
            // TODO: do more complicated mutations
            p.commands.shuffle(&mut self.rng);
            p.inputs.shuffle(&mut self.rng);
            Some(TransactionKind::ProgrammableTransaction(p))
        } else {
            // Other types not supported yet
            None
        }
    }

    fn reset(&mut self, mutations_per_base: u64) {
        self.num_mutations_per_base_left = mutations_per_base;
    }
}
