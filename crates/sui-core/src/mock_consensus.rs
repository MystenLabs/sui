// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::{AuthorityMetrics, AuthorityState, ExecutionEnv};
use crate::checkpoints::CheckpointServiceNoop;
use crate::consensus_adapter::{BlockStatusReceiver, ConsensusClient, SubmitToConsensus};
use crate::consensus_handler::SequencedConsensusTransaction;
use crate::execution_scheduler::ExecutionSchedulerAPI;
use consensus_types::block::BlockRef;
use prometheus::Registry;
use std::sync::{Arc, Weak};
use std::time::Duration;
use sui_types::error::{SuiError, SuiResult};
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::messages_consensus::{
    ConsensusPosition, ConsensusTransaction, ConsensusTransactionKind,
};
use sui_types::transaction::{VerifiedCertificate, VerifiedTransaction};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::debug;

pub struct MockConsensusClient {
    tx_sender: mpsc::Sender<ConsensusTransaction>,
    _consensus_handle: JoinHandle<()>,
}

pub enum ConsensusMode {
    // ConsensusClient does absolutely nothing when receiving a transaction
    Noop,
    // ConsensusClient directly sequences the transaction into the store.
    DirectSequencing,
}

impl MockConsensusClient {
    pub fn new(validator: Weak<AuthorityState>, consensus_mode: ConsensusMode) -> Self {
        let (tx_sender, tx_receiver) = mpsc::channel(1000000);
        let _consensus_handle = Self::run(validator, tx_receiver, consensus_mode);
        Self {
            tx_sender,
            _consensus_handle,
        }
    }

    pub fn run(
        validator: Weak<AuthorityState>,
        tx_receiver: mpsc::Receiver<ConsensusTransaction>,
        consensus_mode: ConsensusMode,
    ) -> JoinHandle<()> {
        tokio::spawn(async move { Self::run_impl(validator, tx_receiver, consensus_mode).await })
    }

    async fn run_impl(
        validator: Weak<AuthorityState>,
        mut tx_receiver: mpsc::Receiver<ConsensusTransaction>,
        consensus_mode: ConsensusMode,
    ) {
        let checkpoint_service = Arc::new(CheckpointServiceNoop {});
        let authority_metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));
        while let Some(tx) = tx_receiver.recv().await {
            let Some(validator) = validator.upgrade() else {
                debug!("validator shut down; exiting MockConsensusClient");
                return;
            };
            let epoch_store = validator.epoch_store_for_testing();
            let env = match consensus_mode {
                ConsensusMode::Noop => ExecutionEnv::new(),
                ConsensusMode::DirectSequencing => {
                    let (_, assigned_versions) = epoch_store
                        .process_consensus_transactions_for_tests(
                            vec![SequencedConsensusTransaction::new_test(tx.clone())],
                            &checkpoint_service,
                            validator.get_object_cache_reader().as_ref(),
                            &authority_metrics,
                            true,
                        )
                        .await
                        .unwrap();
                    let assigned_versions = assigned_versions
                        .0
                        .into_iter()
                        .next()
                        .map(|(_, v)| v)
                        .unwrap_or_default();
                    ExecutionEnv::new().with_assigned_versions(assigned_versions)
                }
            };
            match &tx.kind {
                ConsensusTransactionKind::CertifiedTransaction(tx) => {
                    if tx.is_consensus_tx() {
                        validator.execution_scheduler().enqueue(
                            vec![(
                                VerifiedExecutableTransaction::new_from_certificate(
                                    VerifiedCertificate::new_unchecked(*tx.clone()),
                                )
                                .into(),
                                env,
                            )],
                            &epoch_store,
                        );
                    }
                }
                ConsensusTransactionKind::UserTransaction(tx) => {
                    if tx.is_consensus_tx() {
                        validator.execution_scheduler().enqueue(
                            vec![(
                                VerifiedExecutableTransaction::new_from_consensus(
                                    VerifiedTransaction::new_unchecked(*tx.clone()),
                                    0,
                                )
                                .into(),
                                env,
                            )],
                            &epoch_store,
                        );
                    }
                }
                _ => {}
            }
        }
    }

    fn submit_impl(
        &self,
        transactions: &[ConsensusTransaction],
    ) -> SuiResult<(Vec<ConsensusPosition>, BlockStatusReceiver)> {
        // TODO: maybe support multi-transactions and remove this check
        assert!(transactions.len() == 1);
        let transaction = &transactions[0];
        self.tx_sender
            .try_send(transaction.clone())
            .map_err(|_| SuiError::from("MockConsensusClient channel overflowed"))?;
        // TODO(fastpath): Add some way to simulate consensus positions across blocks
        Ok((
            vec![ConsensusPosition {
                block: BlockRef::MIN,
                index: 0,
            }],
            with_block_status(consensus_core::BlockStatus::Sequenced(BlockRef::MIN)),
        ))
    }
}

impl SubmitToConsensus for MockConsensusClient {
    fn submit_to_consensus(
        &self,
        transactions: &[ConsensusTransaction],
        _epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        self.submit_impl(transactions).map(|_response| ())
    }

    fn submit_best_effort(
        &self,
        transaction: &ConsensusTransaction,
        _epoch_store: &Arc<AuthorityPerEpochStore>,
        _timeout: Duration,
    ) -> SuiResult {
        self.submit_impl(&[transaction.clone()]).map(|_response| ())
    }
}

#[async_trait::async_trait]
impl ConsensusClient for MockConsensusClient {
    async fn submit(
        &self,
        transactions: &[ConsensusTransaction],
        _epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<(Vec<ConsensusPosition>, BlockStatusReceiver)> {
        self.submit_impl(transactions)
    }
}

pub(crate) fn with_block_status(status: consensus_core::BlockStatus) -> BlockStatusReceiver {
    let (tx, rx) = oneshot::channel();
    tx.send(status).ok();
    rx
}
