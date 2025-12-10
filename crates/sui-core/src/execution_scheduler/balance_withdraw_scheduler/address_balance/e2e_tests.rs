// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use move_core_types::language_storage::TypeTag;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::SUI_ACCUMULATOR_ROOT_OBJECT_ID;
use sui_types::accumulator_root::{
    AccumulatorObjId, AccumulatorValue, update_account_balance_for_testing,
};
use sui_types::balance::Balance;
use sui_types::base_types::ObjectID;
use sui_types::digests::TransactionDigest;
use sui_types::execution_params::BalanceWithdrawStatus;
use sui_types::{
    base_types::{SequenceNumber, SuiAddress},
    crypto::{AccountKeyPair, get_account_key_pair},
    executable_transaction::VerifiedExecutableTransaction,
    gas_coin::GAS,
    object::Object,
    transaction::FundsWithdrawalArg,
};
use tokio::sync::mpsc::{self, unbounded_channel};
use tokio::time::timeout;

use super::BalanceSettlement;
use crate::{
    authority::{
        AuthorityState, ExecutionEnv, shared_object_version_manager::Schedulable,
        test_authority_builder::TestAuthorityBuilder,
    },
    execution_scheduler::{ExecutionScheduler, PendingCertificate},
};

struct TestEnv {
    sender: SuiAddress,
    sender_key: AccountKeyPair,
    gas_object: Object,
    account_objects: Vec<ObjectID>,
    rx_ready_certificates: mpsc::UnboundedReceiver<PendingCertificate>,
    scheduler: Arc<ExecutionScheduler>,
    state: Arc<AuthorityState>,
}

#[allow(clippy::disallowed_methods)]
async fn create_test_env(init_balances: BTreeMap<TypeTag, u64>) -> TestEnv {
    let (tx_ready_certificates, rx_ready_certificates) = unbounded_channel();
    let (sender, sender_key) = get_account_key_pair();
    let gas_object = Object::with_owner_for_testing(sender);
    let mut starting_objects: Vec<_> = init_balances
        .into_iter()
        .map(|(type_tag, balance)| {
            let type_tag = Balance::type_(type_tag);
            AccumulatorValue::create_for_testing(sender, type_tag.into(), balance)
        })
        .collect();
    let account_objects = starting_objects.iter().map(|o| o.id()).collect();
    starting_objects.push(gas_object.clone());
    let mut protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
    protocol_config.enable_accumulators_for_testing();
    let state = TestAuthorityBuilder::new()
        .with_protocol_config(protocol_config)
        .with_starting_objects(&starting_objects)
        .build()
        .await;
    let scheduler = Arc::new(ExecutionScheduler::new(
        state.get_object_cache_reader().clone(),
        state.get_child_object_resolver().clone(),
        state.get_transaction_cache_reader().clone(),
        tx_ready_certificates,
        &state.epoch_store_for_testing(),
        state.metrics.clone(),
    ));
    TestEnv {
        sender,
        sender_key,
        gas_object,
        account_objects,
        rx_ready_certificates,
        scheduler,
        state,
    }
}

impl TestEnv {
    fn create_transactions(&self, amounts: Vec<u64>) -> Vec<VerifiedExecutableTransaction> {
        amounts
            .into_iter()
            .enumerate()
            .map(|(idx, amount)| {
                let withdraw =
                    FundsWithdrawalArg::balance_from_sender(amount, GAS::type_tag().into());
                let mut tx_builder = TestTransactionBuilder::new(
                    self.sender,
                    self.gas_object.compute_object_reference(),
                    // Use a unique index to make the transaction digests unique.
                    idx as u64 + 1,
                );
                let tx_data = {
                    let ptb = tx_builder.ptb_builder_mut();
                    ptb.funds_withdrawal(withdraw).unwrap();
                    tx_builder.build()
                };
                VerifiedExecutableTransaction::new_for_testing(tx_data, &self.sender_key)
            })
            .collect()
    }

    fn get_accumulator_object(&self) -> Object {
        self.state
            .get_object_cache_reader()
            .get_object(&SUI_ACCUMULATOR_ROOT_OBJECT_ID)
            .unwrap()
    }

    fn get_accumulator_version(&self) -> SequenceNumber {
        self.get_accumulator_object().version()
    }

    fn enqueue_transactions(&self, transactions: Vec<VerifiedExecutableTransaction>) {
        self.enqueue_transactions_with_version(transactions, self.get_accumulator_version())
    }

    fn enqueue_transactions_with_version(
        &self,
        transactions: Vec<VerifiedExecutableTransaction>,
        version: SequenceNumber,
    ) {
        self.scheduler.enqueue(
            transactions
                .iter()
                .map(|tx| {
                    let mut env = ExecutionEnv::default();
                    env.assigned_versions.accumulator_version = Some(version);
                    (Schedulable::Transaction(tx.clone()), env)
                })
                .collect(),
            &self.state.epoch_store_for_testing(),
        );
    }

    async fn receive_certificate(&mut self) -> Option<PendingCertificate> {
        timeout(Duration::from_secs(1), self.rx_ready_certificates.recv())
            .await
            .ok()?
    }

    async fn expect_withdraw_results(
        &mut self,
        expected_results: BTreeMap<TransactionDigest, BalanceWithdrawStatus>,
    ) {
        let mut results = BTreeMap::new();
        while results.len() < expected_results.len() {
            let cert = self.receive_certificate().await.unwrap();
            results.insert(
                *cert.certificate.digest(),
                cert.execution_env.balance_withdraw_status,
            );
        }
        assert_eq!(results, expected_results);
    }

    fn settle_balances(&mut self, balance_changes: BTreeMap<ObjectID, i128>) {
        let balance_changes: BTreeMap<AccumulatorObjId, i128> = balance_changes
            .into_iter()
            .map(|(object_id, balance_change)| {
                (AccumulatorObjId::new_unchecked(object_id), balance_change)
            })
            .collect();
        let mut accumulator_object = self.get_accumulator_object();
        let next_version = accumulator_object.version().next();
        self.scheduler.settle_balances(BalanceSettlement {
            next_accumulator_version: next_version,
            balance_changes: balance_changes.clone(),
        });
        for (object_id, balance_change) in balance_changes {
            let mut account_object = self
                .state
                .get_object_cache_reader()
                .get_object(object_id.inner())
                .unwrap();
            update_account_balance_for_testing(&mut account_object, balance_change);
            account_object
                .data
                .try_as_move_mut()
                .unwrap()
                .increment_version_to(next_version);
            self.state
                .get_cache_writer()
                .write_object_entry_for_test(account_object);
        }
        accumulator_object
            .data
            .try_as_move_mut()
            .unwrap()
            .increment_version_to(next_version);
        self.state
            .get_cache_writer()
            .write_object_entry_for_test(accumulator_object);
    }
}

#[tokio::test]
async fn test_withdraw_schedule_e2e() {
    telemetry_subscribers::init_for_testing();
    let mut test_env = create_test_env(BTreeMap::from([(GAS::type_tag(), 1000)])).await;
    let transactions: Vec<_> = test_env.create_transactions(vec![400, 600, 1]);
    test_env.enqueue_transactions(transactions.clone());
    test_env
        .expect_withdraw_results(BTreeMap::from([
            (
                *transactions[0].digest(),
                BalanceWithdrawStatus::MaybeSufficient,
            ),
            (
                *transactions[1].digest(),
                BalanceWithdrawStatus::MaybeSufficient,
            ),
            (
                *transactions[2].digest(),
                BalanceWithdrawStatus::Insufficient,
            ),
        ]))
        .await;

    let transactions: Vec<_> = test_env.create_transactions(vec![500, 500]);
    let next_version = test_env.get_accumulator_version().next();
    test_env.enqueue_transactions_with_version(transactions.clone(), next_version);
    assert!(test_env.receive_certificate().await.is_none());

    test_env.settle_balances(BTreeMap::from([(test_env.account_objects[0], -500)]));
    test_env
        .expect_withdraw_results(BTreeMap::from([
            (
                *transactions[0].digest(),
                BalanceWithdrawStatus::MaybeSufficient,
            ),
            (
                *transactions[1].digest(),
                BalanceWithdrawStatus::Insufficient,
            ),
        ]))
        .await;

    test_env.settle_balances(BTreeMap::from([(test_env.account_objects[0], -500)]));

    let transactions = test_env.create_transactions(vec![501]);

    test_env.enqueue_transactions(transactions.clone());

    test_env
        .expect_withdraw_results(BTreeMap::from([(
            *transactions[0].digest(),
            BalanceWithdrawStatus::Insufficient,
        )]))
        .await;
}
