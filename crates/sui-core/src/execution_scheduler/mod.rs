// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::ExecutionEnv;
pub use execution_scheduler_impl::ExecutionScheduler;
use prometheus::IntGauge;
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use tokio::time::Instant;

pub(crate) mod balance_withdraw_scheduler;
pub(crate) mod execution_scheduler_impl;
mod overload_tracker;

// TODO: Cleanup this struct.
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
    // Stores stats about this transaction.
    pub stats: PendingCertificateStats,
    pub executing_guard: Option<ExecutingGuard>,
}

#[derive(Debug)]
pub struct ExecutingGuard {
    num_executing_certificates: IntGauge,
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
