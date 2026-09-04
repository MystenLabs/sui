// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use mysten_common::{assert_reachable, debug_fatal};
use sui_execution::legacy_object_funds::{
    LegacyObjectFundsAccess, LegacyObjectFundsChecker, LegacyObjectFundsOutcome,
};
use sui_types::{
    accumulator_root::AccumulatorObjId, base_types::SequenceNumber, effects::TransactionEffects,
    executable_transaction::VerifiedExecutableTransaction, execution_params::FundsWithdrawStatus,
};
use tokio::time::Instant;
use tracing::{debug, instrument};

use crate::{
    accumulators::{
        funds_read::AccountFundsRead, unsettled_object_withdrawals::UnsettledObjectWithdrawals,
    },
    authority::{ExecutionEnv, authority_per_epoch_store::AuthorityPerEpochStore},
    execution_scheduler::ExecutionScheduler,
};

#[cfg(test)]
mod integration_tests;
pub mod metrics;

struct ObjectFundsAccess<'a> {
    funds_read: &'a dyn AccountFundsRead,
    unsettled: &'a UnsettledObjectWithdrawals,
}

impl LegacyObjectFundsAccess for ObjectFundsAccess<'_> {
    fn get_account_amount_at_version(
        &self,
        account: &AccumulatorObjId,
        version: SequenceNumber,
    ) -> u128 {
        self.funds_read
            .get_account_amount_at_version(account, version)
    }

    fn get_unsettled_withdraw(&self, account: &AccumulatorObjId, version: SequenceNumber) -> u128 {
        self.unsettled
            .get_unsettled_object_withdraw(account, version)
    }

    fn record_unsettled_withdraws(
        &self,
        withdraws: BTreeMap<AccumulatorObjId, u128>,
        version: SequenceNumber,
    ) {
        self.unsettled
            .record_unsettled_withdraws(withdraws, version);
    }
}

/// Adapts the executor-owned legacy decision to the authority's generic re-enqueue mechanism.
#[instrument(level = "debug", skip_all, fields(tx_digest = ?certificate.digest()))]
pub fn should_commit_object_funds_withdraws(
    checker: &Arc<LegacyObjectFundsChecker>,
    certificate: &VerifiedExecutableTransaction,
    effects: &TransactionEffects,
    accumulator_running_max_withdraws: &BTreeMap<AccumulatorObjId, u128>,
    execution_env: &ExecutionEnv,
    funds_read: &Arc<dyn AccountFundsRead>,
    unsettled: &Arc<UnsettledObjectWithdrawals>,
    execution_scheduler: &Arc<ExecutionScheduler>,
    epoch_store: &Arc<AuthorityPerEpochStore>,
) -> bool {
    let access = ObjectFundsAccess {
        funds_read: funds_read.as_ref(),
        unsettled,
    };
    match checker.check_transaction(
        certificate.transaction_data(),
        effects,
        accumulator_running_max_withdraws,
        execution_env.assigned_versions.accumulator_version(),
        epoch_store
            .protocol_config()
            .record_net_unsettled_object_withdraws(),
        epoch_store.get_chain_identifier(),
        &access,
    ) {
        LegacyObjectFundsOutcome::NoCheck => true,
        LegacyObjectFundsOutcome::Commit => {
            assert_reachable!("object funds sufficient");
            debug!("Object funds sufficient, committing effects");
            true
        }
        LegacyObjectFundsOutcome::MissingAccumulatorVersion => {
            debug_fatal!("accumulator_version must be set for a tx with object withdraws");
            false
        }
        LegacyObjectFundsOutcome::Retry(retry) => {
            let scheduler = execution_scheduler.clone();
            let cert = certificate.clone();
            let mut execution_env = execution_env.clone();
            let epoch_store = epoch_store.clone();
            tokio::task::spawn(async move {
                let _ = epoch_store
                    .within_alive_epoch(async move {
                        let tx_digest = cert.digest();
                        match retry.await {
                            FundsWithdrawStatus::MaybeSufficient => {
                                assert_reachable!("object funds maybe sufficient");
                                debug!(?tx_digest, "Object funds possibly sufficient");
                            }
                            FundsWithdrawStatus::Insufficient => {
                                assert_reachable!("object funds insufficient");
                                execution_env = execution_env.with_insufficient_funds();
                                debug!(?tx_digest, "Object funds insufficient");
                            }
                        }
                        scheduler.send_transaction_for_execution(
                            &cert,
                            execution_env,
                            Instant::now(),
                        );
                    })
                    .await;
            });
            false
        }
    }
}
