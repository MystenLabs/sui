// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use move_core_types::language_storage::TypeTag;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::accumulator_root::update_account_balance_for_testing;
use sui_types::base_types::ObjectID;
use sui_types::digests::TransactionDigest;
use sui_types::execution_params::BalanceWithdrawStatus;
use sui_types::transaction::WithdrawTypeParam;
use sui_types::type_input::TypeInput;
use sui_types::SUI_ACCUMULATOR_ROOT_OBJECT_ID;
use sui_types::{
    accumulator_root::create_account_for_testing,
    base_types::{SequenceNumber, SuiAddress},
    crypto::{get_account_key_pair, AccountKeyPair},
    executable_transaction::VerifiedExecutableTransaction,
    gas_coin::GAS,
    object::Object,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::BalanceWithdrawArg,
};
use tokio::sync::mpsc::{self, unbounded_channel};
use tokio::time::timeout;

use crate::execution_scheduler::balance_withdraw_scheduler::BalanceSettlement;
use crate::{
    authority::{
        shared_object_version_manager::Schedulable, test_authority_builder::TestAuthorityBuilder,
        AuthorityState, ExecutionEnv,
    },
    execution_scheduler::{
        ExecutionScheduler, ExecutionSchedulerAPI, ExecutionSchedulerWrapper, PendingCertificate,
    },
};

struct TestEnv {
    sender: SuiAddress,
    sender_key: AccountKeyPair,
    gas_object: Object,
    account_objects: Vec<ObjectID>,
    rx_ready_certificates: mpsc::UnboundedReceiver<PendingCertificate>,
    scheduler: Arc<ExecutionSchedulerWrapper>,
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
            create_account_for_testing(
                sender,
                WithdrawTypeParam::Balance(TypeInput::from(type_tag)),
                balance,
            )
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
    let scheduler = Arc::new(ExecutionSchedulerWrapper::ExecutionScheduler(
        ExecutionScheduler::new(
            state.get_object_cache_reader().clone(),
            state.get_transaction_cache_reader().clone(),
            tx_ready_certificates,
            true,
            state.metrics.clone(),
        ),
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
    fn create_transactions(&self, amounts: Vec<Option<u64>>) -> Vec<VerifiedExecutableTransaction> {
        amounts
            .into_iter()
            .enumerate()
            .map(|(idx, amount)| {
                let withdraw = if let Some(amount) = amount {
                    BalanceWithdrawArg::new_with_amount(amount, GAS::type_tag().into())
                } else {
                    BalanceWithdrawArg::new_with_entire_balance(GAS::type_tag().into())
                };
                let mut ptb = ProgrammableTransactionBuilder::new();
                ptb.balance_withdraw(withdraw).unwrap();
                let tx_data = TestTransactionBuilder::new(
                    self.sender,
                    self.gas_object.compute_object_reference(),
                    // Use a unique index to make the transaction digests unique.
                    idx as u64 + 1,
                )
                .programmable(ptb.finish())
                .build();
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
                    (
                        Schedulable::Withdraw(tx.clone(), version),
                        ExecutionEnv::default(),
                    )
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
                cert.execution_env.withdraw_status,
            );
        }
        assert_eq!(results, expected_results);
    }

    fn settle_balances(&mut self, balance_changes: BTreeMap<ObjectID, i128>) {
        let mut accumulator_object = self.get_accumulator_object();
        let accumulator_version = accumulator_object.version().next();
        self.scheduler.settle_balances(BalanceSettlement {
            accumulator_version,
            balance_changes: balance_changes.clone(),
        });
        for (object_id, balance_change) in balance_changes {
            let mut account_object = self
                .state
                .get_object_cache_reader()
                .get_object(&object_id)
                .unwrap();
            update_account_balance_for_testing(&mut account_object, balance_change);
            account_object
                .data
                .try_as_move_mut()
                .unwrap()
                .increment_version_to(accumulator_version);
            self.state
                .get_cache_writer()
                .write_object_entry_for_test(account_object);
        }
        accumulator_object
            .data
            .try_as_move_mut()
            .unwrap()
            .increment_version_to(accumulator_version);
        self.state
            .get_cache_writer()
            .write_object_entry_for_test(accumulator_object);
    }
}

#[tokio::test]
async fn test_withdraw_schedule_e2e() {
    telemetry_subscribers::init_for_testing();
    let mut test_env = create_test_env(BTreeMap::from([(GAS::type_tag(), 1000)])).await;
    let transactions: Vec<_> = test_env.create_transactions(vec![Some(400), Some(600), Some(1)]);
    test_env.enqueue_transactions(transactions.clone());
    test_env
        .expect_withdraw_results(BTreeMap::from([
            (
                *transactions[0].digest(),
                BalanceWithdrawStatus::SufficientBalance,
            ),
            (
                *transactions[1].digest(),
                BalanceWithdrawStatus::SufficientBalance,
            ),
            (
                *transactions[2].digest(),
                BalanceWithdrawStatus::InsufficientBalance,
            ),
        ]))
        .await;

    let transactions: Vec<_> = test_env.create_transactions(vec![Some(500), Some(500)]);
    let next_version = test_env.get_accumulator_version().next();
    test_env.enqueue_transactions_with_version(transactions.clone(), next_version);
    assert!(test_env.receive_certificate().await.is_none());

    test_env.settle_balances(BTreeMap::from([(test_env.account_objects[0], -500)]));
    test_env
        .expect_withdraw_results(BTreeMap::from([
            (
                *transactions[0].digest(),
                BalanceWithdrawStatus::SufficientBalance,
            ),
            (
                *transactions[1].digest(),
                BalanceWithdrawStatus::InsufficientBalance,
            ),
        ]))
        .await;

    test_env.settle_balances(BTreeMap::from([(test_env.account_objects[0], -500)]));

    let transactions = test_env.create_transactions(vec![None]);

    test_env.enqueue_transactions(transactions.clone());

    test_env
        .expect_withdraw_results(BTreeMap::from([(
            *transactions[0].digest(),
            BalanceWithdrawStatus::InsufficientBalance,
        )]))
        .await;
}
