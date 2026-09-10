// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Testing the integration of the object funds withdraw scheduler with the execution scheduler.
//!
//! Funds-withdrawing transactions are executed through the execution scheduler, in the
//! shape consensus produces: all transactions of one accumulator root version are
//! enqueued together as a single batch (see `execution_scheduler::causal_order`). Since
//! withdrawing mutates the vault, several withdraws at one version need a shared vault,
//! which consensus sequences within the batch.

use std::{cell::Cell, sync::Arc};

use fastcrypto::ed25519::Ed25519KeyPair;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::{FundSource, TestTransactionBuilder};
use sui_types::{
    SUI_ACCUMULATOR_ROOT_OBJECT_ID, TypeTag,
    accumulator_root::AccumulatorValue,
    balance::Balance,
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    crypto::get_account_key_pair,
    effects::{TransactionEffects, TransactionEffectsAPI},
    executable_transaction::VerifiedExecutableTransaction,
    execution_status::{ExecutionErrorKind, ExecutionFailure, ExecutionStatus},
    gas_coin::GAS,
    object::{Object, Owner},
};

use crate::authority::{
    AuthorityState, ExecutionEnv, authority_per_epoch_store::AuthorityPerEpochStore,
    shared_object_version_manager::AssignedVersions, test_authority_builder::TestAuthorityBuilder,
};

/// Gas coins available to scheduled transactions; each takes a fresh one so a batch
/// never reuses an owned input.
const GAS_POOL_SIZE: usize = 16;

struct TestEnv {
    authority: Arc<AuthorityState>,
    epoch_store: Arc<AuthorityPerEpochStore>,
    sender: SuiAddress,
    keypair: Ed25519KeyPair,
    /// Gas for the directly executed setup transactions, chained through their effects.
    gas_obj: ObjectID,
    gas_pool: Vec<ObjectID>,
    next_gas: Cell<usize>,
    package_id: ObjectID,
    /// An owned vault, for single-transaction version groups.
    vault_obj: ObjectID,
    /// A shared vault, for version groups with several withdraws.
    shared_vault: ObjectID,
    shared_vault_initial_version: SequenceNumber,
}

impl TestEnv {
    pub async fn new() -> Self {
        Self::new_impl(true).await
    }

    /// Test environment with the legacy behavior of recording running max withdraws
    /// (instead of net withdraws) as unsettled.
    pub async fn new_with_legacy_unsettled_withdraws() -> Self {
        Self::new_impl(false).await
    }

    async fn new_impl(record_net_unsettled_object_withdraws: bool) -> Self {
        let mut protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
        protocol_config.set_record_net_unsettled_object_withdraws_for_testing(
            record_net_unsettled_object_withdraws,
        );
        protocol_config.set_check_object_funds_withdraw_in_execution_for_testing(false);

        let (sender, keypair) = get_account_key_pair();
        let gas_obj = Object::with_owner_for_testing(sender);
        let gas_pool: Vec<_> = (0..GAS_POOL_SIZE)
            .map(|_| Object::with_owner_for_testing(sender))
            .collect();
        let mut starting_objects = vec![gas_obj.clone()];
        starting_objects.extend(gas_pool.iter().cloned());

        let authority = TestAuthorityBuilder::new()
            .with_protocol_config(protocol_config)
            .with_starting_objects(&starting_objects)
            .build()
            .await;
        let epoch_store = authority.epoch_store_for_testing().clone();

        let gas = gas_obj.compute_object_reference();
        let rgp = epoch_store.reference_gas_price();
        let tx = TestTransactionBuilder::new(sender, gas, rgp)
            .publish_examples("object_balance")
            .await
            .build();
        let cert = VerifiedExecutableTransaction::new_for_testing(tx, &keypair);
        let (effects, ..) = authority
            .try_execute_immediately(&cert, ExecutionEnv::new(), &epoch_store)
            .unwrap();
        assert!(effects.status().is_ok());
        let package_id = effects
            .created()
            .into_iter()
            .find(|(_, owner)| owner.is_immutable())
            .unwrap()
            .0
            .0;
        let gas = effects.gas_object().unwrap().0;

        let tx = TestTransactionBuilder::new(sender, gas, rgp)
            .move_call(package_id, "object_balance", "new_owned", vec![])
            .build();
        let cert = VerifiedExecutableTransaction::new_for_testing(tx, &keypair);
        let (effects, ..) = authority
            .try_execute_immediately(&cert, ExecutionEnv::new(), &epoch_store)
            .unwrap();
        assert!(effects.status().is_ok());
        let vault_obj = effects.created().into_iter().next().unwrap().0;
        let gas = effects.gas_object().unwrap().0;

        let tx = TestTransactionBuilder::new(sender, gas, rgp)
            .move_call(package_id, "object_balance", "new_shared", vec![])
            .build();
        let cert = VerifiedExecutableTransaction::new_for_testing(tx, &keypair);
        let (effects, ..) = authority
            .try_execute_immediately(&cert, ExecutionEnv::new(), &epoch_store)
            .unwrap();
        assert!(effects.status().is_ok());
        let (shared_vault, shared_vault_initial_version) = effects
            .created()
            .into_iter()
            .find_map(|(oref, owner)| match owner {
                Owner::Shared {
                    initial_shared_version,
                } => Some((oref.0, initial_shared_version)),
                _ => None,
            })
            .unwrap();
        let gas = effects.gas_object().unwrap().0;

        Self {
            authority,
            epoch_store,
            sender,
            keypair,
            gas_obj: gas.0,
            gas_pool: gas_pool.iter().map(|o| o.id()).collect(),
            next_gas: Cell::new(0),
            package_id,
            vault_obj: vault_obj.0,
            shared_vault,
            shared_vault_initial_version,
        }
    }

    /// Creates another vault object account, in addition to the default one.
    pub async fn new_vault(&self) -> ObjectID {
        let gas = self.oref(&self.gas_obj);
        let tx = TestTransactionBuilder::new(self.sender, gas, self.rgp())
            .move_call(self.package_id, "object_balance", "new_owned", vec![])
            .build();
        let cert = VerifiedExecutableTransaction::new_for_testing(tx, &self.keypair);
        let (effects, ..) = self
            .authority
            .try_execute_immediately(&cert, ExecutionEnv::new(), &self.epoch_store)
            .unwrap();
        assert!(effects.status().is_ok());
        effects.created().into_iter().next().unwrap().0.0
    }

    pub fn oref(&self, object_id: &ObjectID) -> ObjectRef {
        self.authority
            .get_object(object_id)
            .unwrap()
            .compute_object_reference()
    }

    /// A gas coin not used by any earlier transaction.
    pub fn fresh_gas(&self) -> ObjectRef {
        let i = self.next_gas.get();
        self.next_gas.set(i + 1);
        self.oref(&self.gas_pool[i])
    }

    pub fn rgp(&self) -> u64 {
        self.epoch_store.reference_gas_price()
    }

    pub async fn fund_address(&self, address: SuiAddress, amount: u64) {
        let gas = self.oref(&self.gas_obj);
        let tx = TestTransactionBuilder::new(self.sender, gas, self.rgp())
            .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(amount, address)])
            .build();
        let cert = VerifiedExecutableTransaction::new_for_testing(tx, &self.keypair);

        let (effects, ..) = self
            .authority
            .try_execute_immediately(&cert, ExecutionEnv::new(), &self.epoch_store)
            .unwrap();
        assert!(effects.status().is_ok());

        self.authority
            .settle_accumulator_for_testing(&[effects], None)
            .await;
    }

    pub fn vault_balance(&self, vault: ObjectID, type_tag: TypeTag) -> u128 {
        let account_id =
            AccumulatorValue::get_field_id(vault.into(), &Balance::type_tag(type_tag)).unwrap();
        let balance_read = self.authority.get_account_funds_read();
        balance_read.get_latest_account_amount(&account_id)
    }

    /// Builds a transaction that, for each `(amount, recipient)`, withdraws `amount` from
    /// the shared vault object account and deposits it to the recipient's address balance.
    pub fn shared_vault_withdraw_tx(
        &self,
        transfers: &[(u64, SuiAddress)],
    ) -> VerifiedExecutableTransaction {
        let mut builder = TestTransactionBuilder::new(self.sender, self.fresh_gas(), self.rgp());
        for (amount, recipient) in transfers {
            builder = builder.transfer_sui_to_address_balance(
                FundSource::object_fund_shared(
                    self.package_id,
                    self.shared_vault,
                    self.shared_vault_initial_version,
                ),
                vec![(*amount, *recipient)],
            );
        }
        VerifiedExecutableTransaction::new_for_testing(builder.build(), &self.keypair)
    }

    /// Enqueues `certs` as one version group at the current accumulator root version,
    /// waits for their effects, and asserts each outcome. Also asserts that exactly the
    /// insufficient transactions went through the checker's pending path: a sufficient
    /// transaction executes on its first attempt, an insufficient one is retried once.
    pub async fn execute_batch(
        &self,
        certs: &[VerifiedExecutableTransaction],
        expected: &[Expect],
    ) -> Vec<TransactionEffects> {
        assert_eq!(certs.len(), expected.len());
        let pending_before = self.pending_check_count();

        let assigned = self
            .epoch_store
            .assign_shared_object_versions_for_tests(
                self.authority.get_object_cache_reader().as_ref(),
                certs,
            )
            .unwrap()
            .into_map();
        let accumulator_version = self.oref(&SUI_ACCUMULATOR_ROOT_OBJECT_ID).1;
        let batch = certs
            .iter()
            .map(|cert| {
                let shared_versions = assigned[&cert.key()].shared_object_versions.clone();
                (
                    cert.clone().into(),
                    ExecutionEnv::new().with_assigned_versions(AssignedVersions::new_for_testing(
                        shared_versions,
                        Some(accumulator_version),
                    )),
                )
            })
            .collect();
        self.authority
            .execution_scheduler()
            .enqueue(batch, &self.epoch_store);

        let mut all_effects = Vec::with_capacity(certs.len());
        for (cert, expect) in certs.iter().zip(expected) {
            let effects = self
                .authority
                .notify_read_effects_for_testing("test", *cert.digest())
                .await;
            match expect {
                Expect::Ok => assert_ok(&effects),
                Expect::Insufficient => assert_insufficient(&effects),
            }
            all_effects.push(effects);
        }

        // The pending check is recorded after the retry is re-sent, so it can trail the
        // retried transaction's effects briefly.
        let expected_pending = expected
            .iter()
            .filter(|e| matches!(e, Expect::Insufficient))
            .count() as u64;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while self.pending_check_count() < pending_before + expected_pending
            && std::time::Instant::now() < deadline
        {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(
            self.pending_check_count() - pending_before,
            expected_pending,
            "pending object funds checks"
        );
        all_effects
    }

    /// Number of object funds checks that have gone through the pending path so far.
    fn pending_check_count(&self) -> u64 {
        self.authority
            .object_funds_checker
            .load()
            .as_ref()
            .expect("object funds checker must be initialized")
            .metrics()
            .pending_check_latency
            .get_sample_count()
    }
}

/// Expected outcome of a transaction in an `execute_batch` call.
#[derive(Clone, Copy)]
pub enum Expect {
    Ok,
    Insufficient,
}

fn assert_ok(effects: &TransactionEffects) {
    assert!(effects.status().is_ok(), "{:?}", effects.status());
}

/// The object funds check rejected the transaction with InsufficientFundsForWithdraw.
fn assert_insufficient(effects: &TransactionEffects) {
    assert!(
        matches!(
            effects.status(),
            ExecutionStatus::Failure(ExecutionFailure {
                error: ExecutionErrorKind::InsufficientFundsForWithdraw,
                ..
            })
        ),
        "{:?}",
        effects.status()
    );
}

#[tokio::test]
async fn test_object_withdraw_basic_flow() {
    let env = TestEnv::new().await;

    env.fund_address(env.vault_obj.into(), 1000).await;

    let tx = TestTransactionBuilder::new(env.sender, env.fresh_gas(), env.rgp())
        .transfer_sui_to_address_balance(
            FundSource::object_fund_owned(env.package_id, env.oref(&env.vault_obj)),
            vec![(1000, env.sender)],
        )
        .build();
    let cert = VerifiedExecutableTransaction::new_for_testing(tx, &env.keypair);

    env.execute_batch(&[cert], &[Expect::Ok]).await;
}

#[tokio::test]
async fn test_object_withdraw_multiple_withdraws() {
    let env = TestEnv::new().await;
    let vault = env.shared_vault;

    env.fund_address(vault.into(), 1000).await;

    // Withdraw from the same object account 3 times, each 300.
    // All withdraws should be sufficient.
    let certs: Vec<_> = (0..3)
        .map(|_| env.shared_vault_withdraw_tx(&[(300, env.sender)]))
        .collect();
    let all_effects = env.execute_batch(&certs, &[Expect::Ok; 3]).await;
    env.authority
        .settle_accumulator_for_testing(&all_effects, None)
        .await;

    assert_eq!(env.vault_balance(vault, GAS::type_tag()), 1000 - 300 * 3);

    // Withdraw from the same object account 3 times, each 40.
    // The first 2 withdraws should be sufficient, the last one should be insufficient.
    // This test exercises the case where we have to track unsettled balance withdraws from the same consensus commit.
    let certs: Vec<_> = (0..3)
        .map(|_| env.shared_vault_withdraw_tx(&[(40, env.sender)]))
        .collect();
    let all_effects = env
        .execute_batch(&certs, &[Expect::Ok, Expect::Ok, Expect::Insufficient])
        .await;
    env.authority
        .settle_accumulator_for_testing(&all_effects, None)
        .await;

    assert_eq!(
        env.vault_balance(vault, GAS::type_tag()),
        1000 - 300 * 3 - 40 * 2
    );
}

#[tokio::test]
async fn test_object_withdraw_and_deposit_same_transaction() {
    telemetry_subscribers::init_for_testing();
    let env = TestEnv::new().await;
    let vault: SuiAddress = env.shared_vault.into();
    env.fund_address(vault, 2).await;

    let certs = vec![
        // Withdraw 3 and deposit 3 back to the same object account. Even though this nets
        // out to 0, the running max withdraw of 3 exceeds the balance of 2, so it fails.
        env.shared_vault_withdraw_tx(&[(3, vault)]),
        // Withdraw 2 and deposit 2 back, twice within the same transaction. The running
        // net withdraw never exceeds 2, so this succeeds.
        env.shared_vault_withdraw_tx(&[(2, vault), (2, vault)]),
        // The previous transaction's withdraws netted out to 0, so the full balance of 2
        // is still available at the same version.
        env.shared_vault_withdraw_tx(&[(1, vault)]),
        // Withdraw the full balance of 2 without depositing back.
        env.shared_vault_withdraw_tx(&[(2, env.sender)]),
        // The balance is now fully reserved; even a withdraw of 1 that deposits back
        // must fail.
        env.shared_vault_withdraw_tx(&[(1, vault)]),
    ];
    let all_effects = env
        .execute_batch(
            &certs,
            &[
                Expect::Insufficient,
                Expect::Ok,
                Expect::Ok,
                Expect::Ok,
                Expect::Insufficient,
            ],
        )
        .await;

    // Settlement applies the net amounts: only the full-balance withdraw of 2
    // actually deducted funds.
    env.authority
        .settle_accumulator_for_testing(&all_effects, None)
        .await;
    assert_eq!(env.vault_balance(env.shared_vault, GAS::type_tag()), 0);
}

#[tokio::test]
async fn test_object_net_deposit_same_transaction() {
    telemetry_subscribers::init_for_testing();
    let env = TestEnv::new().await;
    let vault: SuiAddress = env.shared_vault.into();
    env.fund_address(vault, 2).await;

    // In one transaction, withdraw 2 from the vault and deposit it back, plus deposit
    // 3 more from a coin. The vault's folded accumulator event is a net deposit, which
    // is recorded as 0 unsettled withdraw, while the withdraw of 2 is still checked
    // against the running max.
    let gas = env.fresh_gas();
    let tx = TestTransactionBuilder::new(env.sender, gas, env.rgp())
        .transfer_sui_to_address_balance(
            FundSource::object_fund_shared(
                env.package_id,
                env.shared_vault,
                env.shared_vault_initial_version,
            ),
            vec![(2, vault)],
        )
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(3, vault)])
        .build();
    let certs = vec![
        VerifiedExecutableTransaction::new_for_testing(tx, &env.keypair),
        // The net deposit consumed no unsettled balance: the full balance of 2 is still
        // available at the same version.
        env.shared_vault_withdraw_tx(&[(2, env.sender)]),
        // But the unsettled deposit of 3 is not credited before settlement.
        env.shared_vault_withdraw_tx(&[(1, env.sender)]),
    ];
    let all_effects = env
        .execute_batch(&certs, &[Expect::Ok, Expect::Ok, Expect::Insufficient])
        .await;

    // After settlement the net deposit materializes: 2 + 3 - 2 = 3.
    env.authority
        .settle_accumulator_for_testing(&all_effects, None)
        .await;
    assert_eq!(env.vault_balance(env.shared_vault, GAS::type_tag()), 3);
}

#[tokio::test]
async fn test_object_zero_amount_withdraw() {
    // A zero-amount object-fund withdraw emits a single Split(0) accumulator event.
    // It survives effects folding as a Split (the fold's Merge tie-break only applies
    // to accounts with multiple writes), but creates no running max entry. It must be
    // skipped when recording unsettled withdraws instead of tripping the recording
    // invariant check.
    telemetry_subscribers::init_for_testing();
    let env = TestEnv::new().await;
    let vault: SuiAddress = env.vault_obj.into();
    let zero_vault = env.new_vault().await;
    env.fund_address(vault, 2).await;

    // One transaction: withdraw 0 from zero_vault and 2 from the funded vault. The
    // positive withdraw makes the running max map non-empty, so the zero withdraw
    // reaches the unsettled recording path.
    let tx = TestTransactionBuilder::new(env.sender, env.fresh_gas(), env.rgp())
        .transfer_sui_to_address_balance(
            FundSource::object_fund_owned(env.package_id, env.oref(&zero_vault)),
            vec![(0, env.sender)],
        )
        .transfer_sui_to_address_balance(
            FundSource::object_fund_owned(env.package_id, env.oref(&env.vault_obj)),
            vec![(2, env.sender)],
        )
        .build();
    let cert = VerifiedExecutableTransaction::new_for_testing(tx, &env.keypair);
    let effects = env.execute_batch(&[cert], &[Expect::Ok]).await;

    env.authority
        .settle_accumulator_for_testing(&effects, None)
        .await;
    assert_eq!(env.vault_balance(env.vault_obj, GAS::type_tag()), 0);
}

#[tokio::test]
async fn test_object_withdraw_and_deposit_same_transaction_legacy() {
    // With record_net_unsettled_object_withdraws disabled, the running max withdraw
    // (rather than the net withdraw) is recorded as unsettled, so a transaction
    // whose withdraws net out to 0 still blocks subsequent withdraws at the same
    // accumulator version.
    telemetry_subscribers::init_for_testing();
    let env = TestEnv::new_with_legacy_unsettled_withdraws().await;
    let vault: SuiAddress = env.shared_vault.into();
    env.fund_address(vault, 2).await;

    let certs = vec![
        // Withdraw 2 and deposit 2 back to the same object account.
        env.shared_vault_withdraw_tx(&[(2, vault)]),
        // Even though the previous transaction netted out to 0, its running max withdraw
        // of 2 is recorded as unsettled, so no balance remains available.
        env.shared_vault_withdraw_tx(&[(1, vault)]),
    ];
    env.execute_batch(&certs, &[Expect::Ok, Expect::Insufficient])
        .await;
}
