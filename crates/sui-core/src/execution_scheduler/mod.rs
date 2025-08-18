// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    authority::{
        authority_per_epoch_store::AuthorityPerEpochStore,
        shared_object_version_manager::Schedulable, AuthorityMetrics, ExecutionEnv,
    },
    execution_cache::{ObjectCacheRead, TransactionCacheRead},
    execution_scheduler::balance_withdraw_scheduler::BalanceSettlement,
};
use enum_dispatch::enum_dispatch;
use execution_scheduler_impl::ExecutionScheduler;
use prometheus::IntGauge;
use std::{collections::BTreeSet, sync::Arc};
use sui_config::node::AuthorityOverloadConfig;
use sui_types::{
    error::SuiResult,
    executable_transaction::VerifiedExecutableTransaction,
    storage::{ChildObjectResolver, InputKey},
    transaction::SenderSignedData,
};
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::Instant;
use transaction_manager::TransactionManager;

pub(crate) mod balance_withdraw_scheduler;
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

    fn settle_balances(&self, settlement: BalanceSettlement);

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
        child_object_resolver: Arc<dyn ChildObjectResolver + Send + Sync>,
        transaction_cache_read: Arc<dyn TransactionCacheRead>,
        tx_ready_certificates: UnboundedSender<PendingCertificate>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        _is_fullnode: bool,
        metrics: Arc<AuthorityMetrics>,
    ) -> Self {
        // Execution scheduler is enabled by default unless ENABLE_TRANSACTION_MANAGER is explicitly set.
        let enable_execution_scheduler = std::env::var("ENABLE_TRANSACTION_MANAGER").is_err();
        if enable_execution_scheduler {
            let enable_accumulators = epoch_store.accumulators_enabled();
            Self::ExecutionScheduler(ExecutionScheduler::new(
                object_cache_read,
                child_object_resolver,
                transaction_cache_read,
                tx_ready_certificates,
                enable_accumulators,
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
