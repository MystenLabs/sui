// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    authority::{
        authority_per_epoch_store::AuthorityPerEpochStore,
        shared_object_version_manager::Schedulable, AuthorityMetrics, ExecutionEnv,
    },
    execution_cache::{ObjectCacheRead, TransactionCacheRead},
};
use enum_dispatch::enum_dispatch;
use execution_scheduler_impl::ExecutionScheduler;
use mysten_common::in_test_configuration;
use prometheus::IntGauge;
use rand::Rng;
use std::{collections::BTreeSet, sync::Arc};
use sui_config::node::AuthorityOverloadConfig;
use sui_protocol_config::Chain;
use sui_types::{
    error::SuiResult, executable_transaction::VerifiedExecutableTransaction, storage::InputKey,
    transaction::SenderSignedData,
};
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::Instant;
use transaction_manager::TransactionManager;

mod balance_withdraw_scheduler;
pub(crate) mod execution_scheduler_impl;
mod overload_tracker;
pub(crate) mod transaction_manager;

#[derive(Clone, Debug)]
pub struct PendingCertificateStats {
    // The time this certificate enters execution scheduler.
    #[allow(unused)]
    pub enqueue_time: Instant,
    // The time this certificate becomes ready for execution.
    pub ready_time: Option<Instant>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum SchedulingSource {
    MysticetiFastPath,
    NonFastPath,
}

#[derive(Debug)]
pub struct PendingCertificate {
    // Certified transaction to be executed.
    pub certificate: VerifiedExecutableTransaction,
    // Environment in which the transaction will be executed.
    pub execution_env: ExecutionEnv,
    // The input object this certificate is waiting for to become available in order to be executed.
    // This is only used by TransactionManager.
    pub waiting_input_objects: BTreeSet<InputKey>,
    // Stores stats about this transaction.
    pub stats: PendingCertificateStats,
    pub executing_guard: Option<ExecutingGuard>,
}

#[derive(Debug)]
pub struct ExecutingGuard {
    num_executing_certificates: IntGauge,
}

#[enum_dispatch]
pub trait ExecutionSchedulerAPI {
    fn enqueue_transactions(
        &self,
        certs: Vec<(VerifiedExecutableTransaction, ExecutionEnv)>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    );

    fn enqueue(
        &self,
        certs: Vec<(Schedulable, ExecutionEnv)>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    );

    fn check_execution_overload(
        &self,
        overload_config: &AuthorityOverloadConfig,
        tx_data: &SenderSignedData,
    ) -> SuiResult;

    // Returns the number of transactions pending or being executed right now.
    fn num_pending_certificates(&self) -> usize;

    // Verify TM has no pending item for tests.
    #[cfg(test)]
    fn check_empty_for_testing(&self);
}

#[enum_dispatch(ExecutionSchedulerAPI)]
pub enum ExecutionSchedulerWrapper {
    ExecutionScheduler(ExecutionScheduler),
    TransactionManager(TransactionManager),
}

impl ExecutionSchedulerWrapper {
    pub fn new(
        object_cache_read: Arc<dyn ObjectCacheRead>,
        transaction_cache_read: Arc<dyn TransactionCacheRead>,
        tx_ready_certificates: UnboundedSender<PendingCertificate>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        is_fullnode: bool,
        metrics: Arc<AuthorityMetrics>,
    ) -> Self {
        // If Mysticeti fastpath is enabled, we must use ExecutionScheduler.
        // This is because TransactionManager prohibits enqueueing the same transaction twice,
        // which we need in Mysticeti fastpath in order to finalize a transaction that
        // was previously executed through fastpaht certification.
        // In tests, we flip a coin to decide whether to use ExecutionScheduler or TransactionManager,
        // so that both can be tested.
        // In prod, we use ExecutionScheduler in devnet, and in testnet fullnodes.
        // In other networks, we use TransactionManager by default.
        let enable_execution_scheduler = if epoch_store.protocol_config().mysticeti_fastpath()
            || std::env::var("ENABLE_EXECUTION_SCHEDULER").is_ok()
        {
            true
        } else if std::env::var("ENABLE_TRANSACTION_MANAGER").is_ok() {
            false
        } else if in_test_configuration() {
            rand::thread_rng().gen_bool(0.5)
        } else {
            let chain = epoch_store.get_chain_identifier().chain();
            chain == Chain::Unknown || (chain == Chain::Testnet && is_fullnode)
        };
        if enable_execution_scheduler {
            Self::ExecutionScheduler(ExecutionScheduler::new(
                object_cache_read,
                transaction_cache_read,
                tx_ready_certificates,
                metrics,
            ))
        } else {
            Self::TransactionManager(TransactionManager::new(
                object_cache_read,
                transaction_cache_read,
                epoch_store,
                tx_ready_certificates,
                metrics,
            ))
        }
    }
}

impl ExecutingGuard {
    pub fn new(num_executing_certificates: IntGauge) -> Self {
        num_executing_certificates.inc();
        Self {
            num_executing_certificates,
        }
    }
}

impl Drop for ExecutingGuard {
    fn drop(&mut self) {
        self.num_executing_certificates.dec();
    }
}
