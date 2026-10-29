// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Checks that native settlement writes the same objects as the Move settlement.
//!
//! The same user transactions run on two authorities that differ only in
//! `native_accumulator_settlement`. After each settlement, the effects and written objects of the
//! two must match in everything except values derived from transaction digests.

use std::sync::Arc;

use fastcrypto::ed25519::Ed25519KeyPair;
use itertools::Itertools;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::{FundSource, TestTransactionBuilder};
use sui_types::{
    SUI_ACCUMULATOR_ROOT_OBJECT_ID, TypeTag,
    accumulator_root::AccumulatorValue,
    balance::Balance,
    base_types::{ObjectRef, SuiAddress},
    crypto::get_account_key_pair,
    effects::{TransactionEffects, TransactionEffectsAPI},
    executable_transaction::VerifiedExecutableTransaction,
    gas_coin::GAS,
    object::Object,
    transaction::{TransactionData, TransactionDataAPI, TransactionKind},
};

use crate::authority::{
    AuthorityState, ExecutionEnv, test_authority_builder::TestAuthorityBuilder,
};

struct Account {
    address: SuiAddress,
    keypair: Ed25519KeyPair,
    gas: ObjectRef,
}

struct TestEnv {
    move_authority: Arc<AuthorityState>,
    native_authority: Arc<AuthorityState>,
    checkpoint_seq: u64,
}

impl TestEnv {
    async fn new(accounts: &[(SuiAddress, &Object)]) -> Self {
        let starting_objects: Vec<_> = accounts.iter().map(|(_, o)| (*o).clone()).collect();
        let build = |native: bool| {
            let mut protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
            protocol_config.set_native_accumulator_settlement_for_testing(native);
            // Small chunks, so that a checkpoint settles in several transactions.
            protocol_config.set_max_updates_per_settlement_txn_for_testing(2);
            let starting_objects = starting_objects.clone();
            async move {
                TestAuthorityBuilder::new()
                    .with_protocol_config(protocol_config)
                    .with_starting_objects(&starting_objects)
                    .build()
                    .await
            }
        };
        Self {
            move_authority: build(false).await,
            native_authority: build(true).await,
            checkpoint_seq: 1,
        }
    }

    async fn rgp(&self) -> u64 {
        self.move_authority
            .epoch_store_for_testing()
            .reference_gas_price()
    }

    /// Executes `tx` on both authorities and returns its effects, which must be identical.
    async fn execute(&self, tx: TransactionData, account: &mut Account) -> TransactionEffects {
        let cert = VerifiedExecutableTransaction::new_for_testing(tx, &account.keypair);
        let mut all_effects = vec![];
        for authority in [&self.move_authority, &self.native_authority] {
            let (effects, _) = authority
                .try_execute_immediately(
                    &cert,
                    ExecutionEnv::new(),
                    &authority.epoch_store_for_testing(),
                )
                .unwrap();
            assert!(effects.status().is_ok(), "{:?}", effects.status());
            all_effects.push(effects);
        }
        assert_eq!(all_effects[0], all_effects[1]);
        account.gas = all_effects[0].gas_object().unwrap().0;
        all_effects.pop().unwrap()
    }

    /// Settles `effects` on both authorities, compares the results, and returns the number of
    /// settlement transactions, including the barrier.
    async fn settle_and_compare(&mut self, effects: &[TransactionEffects]) -> usize {
        let checkpoint_seq = Some(self.checkpoint_seq);
        self.checkpoint_seq += 1;

        let move_txns = self
            .move_authority
            .settle_accumulator_for_testing(effects, checkpoint_seq)
            .await;
        let native_txns = self
            .native_authority
            .settle_accumulator_for_testing(effects, checkpoint_seq)
            .await;
        assert_eq!(move_txns.len(), native_txns.len());

        for ((move_tx, _), (native_tx, _)) in move_txns.iter().zip_eq(&native_txns) {
            assert!(matches!(
                move_tx.transaction_data().kind(),
                TransactionKind::ProgrammableSystemTransaction(_)
            ));
            assert!(matches!(
                native_tx.transaction_data().kind(),
                TransactionKind::AccumulatorSettlement(_)
            ));
            let move_effects = self.executed_effects(&self.move_authority, move_tx);
            let native_effects = self.executed_effects(&self.native_authority, native_tx);
            self.compare_effects(&move_effects, &native_effects);
        }
        move_txns.len()
    }

    fn executed_effects(
        &self,
        authority: &AuthorityState,
        tx: &VerifiedExecutableTransaction,
    ) -> TransactionEffects {
        authority
            .get_transaction_cache_reader()
            .get_executed_effects(tx.digest())
            .unwrap()
    }

    /// Compares effects and written objects, ignoring transaction digests and the object
    /// digests that depend on them through `previous_transaction`.
    fn compare_effects(
        &self,
        move_effects: &TransactionEffects,
        native_effects: &TransactionEffects,
    ) {
        assert_eq!(move_effects.status(), native_effects.status());
        assert_eq!(
            move_effects.gas_cost_summary(),
            native_effects.gas_cost_summary()
        );
        assert_eq!(
            move_effects.lamport_version(),
            native_effects.lamport_version()
        );
        assert!(move_effects.events_digest().is_none());
        assert!(native_effects.events_digest().is_none());

        let unchanged = |effects: &TransactionEffects| {
            effects
                .unchanged_consensus_objects()
                .into_iter()
                .map(|(id, kind)| {
                    (
                        id,
                        format!("{kind:?}").split('(').next().unwrap().to_owned(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(unchanged(move_effects), unchanged(native_effects));

        let move_changes = move_effects.object_changes();
        let native_changes = native_effects.object_changes();
        assert_eq!(
            move_changes.len(),
            native_changes.len(),
            "move: {move_changes:#?}\nnative: {native_changes:#?}"
        );
        for (m, n) in move_changes.iter().zip_eq(&native_changes) {
            assert_eq!(m.id, n.id);
            assert_eq!(m.input_version, n.input_version);
            assert_eq!(m.output_version, n.output_version);
            assert_eq!(m.id_operation, n.id_operation);
            assert_eq!(m.output_digest.is_some(), n.output_digest.is_some());

            let Some(version) = m.output_version else {
                continue;
            };
            let read = |authority: &AuthorityState| {
                authority
                    .get_object_store()
                    .get_object_by_key(&m.id, version)
                    .unwrap()
            };
            let move_object = read(&self.move_authority);
            let native_object = read(&self.native_authority);
            assert_eq!(move_object.data, native_object.data, "object {}", m.id);
            assert_eq!(move_object.owner, native_object.owner, "object {}", m.id);
            assert_eq!(
                move_object.storage_rebate, native_object.storage_rebate,
                "object {}",
                m.id
            );
        }
    }

    fn balance(&self, owner: SuiAddress) -> u128 {
        let id =
            AccumulatorValue::get_field_id(owner, &Balance::type_tag(GAS::type_tag())).unwrap();
        let read = |authority: &AuthorityState| {
            authority
                .get_object_store()
                .get_object(id.inner())
                .map_or(0, |object| {
                    AccumulatorValue::try_from(object.data.try_as_move().unwrap())
                        .unwrap()
                        .as_u128()
                        .unwrap()
                })
        };
        let balance = read(&self.move_authority);
        assert_eq!(balance, read(&self.native_authority));
        balance
    }

    fn accumulator_exists(&self, owner: SuiAddress) -> bool {
        let id =
            AccumulatorValue::get_field_id(owner, &Balance::type_tag(GAS::type_tag())).unwrap();
        let exists = |authority: &AuthorityState| {
            authority
                .get_object_store()
                .get_object(id.inner())
                .is_some()
        };
        let exists_move = exists(&self.move_authority);
        assert_eq!(exists_move, exists(&self.native_authority));
        exists_move
    }
}

fn new_account() -> (Account, Object) {
    let (address, keypair) = get_account_key_pair();
    let gas = Object::with_owner_for_testing(address);
    let account = Account {
        address,
        keypair,
        gas: gas.compute_object_reference(),
    };
    (account, gas)
}

fn sui_type() -> TypeTag {
    GAS::type_tag()
}

#[tokio::test]
async fn test_native_settlement_matches_move() {
    let (mut alice, alice_gas) = new_account();
    let (mut bob, bob_gas) = new_account();
    let carol = get_account_key_pair().0;
    let dave = get_account_key_pair().0;
    let mut env = TestEnv::new(&[(alice.address, &alice_gas), (bob.address, &bob_gas)]).await;
    let rgp = env.rgp().await;

    // Checkpoint 1: deposits create four accumulators.
    let tx = TestTransactionBuilder::new(alice.address, alice.gas, rgp)
        .transfer_sui_to_address_balance(
            FundSource::coin(alice.gas),
            vec![(1000, alice.address), (2000, bob.address), (3000, carol)],
        )
        .build();
    let e1 = env.execute(tx, &mut alice).await;
    let tx = TestTransactionBuilder::new(bob.address, bob.gas, rgp)
        .transfer_sui_to_address_balance(FundSource::coin(bob.gas), vec![(4000, dave)])
        .build();
    let e2 = env.execute(tx, &mut bob).await;
    // Four updates in chunks of two, plus the barrier.
    assert_eq!(env.settle_and_compare(&[e1, e2]).await, 3);
    assert_eq!(env.balance(alice.address), 1000);
    assert_eq!(env.balance(bob.address), 2000);
    assert_eq!(env.balance(carol), 3000);
    assert_eq!(env.balance(dave), 4000);

    // Checkpoint 2:
    // - alice withdraws her whole balance, which deletes her accumulator;
    // - bob withdraws part of his balance;
    // - carol and dave receive deposits;
    // - a deposit and a withdrawal for bob net out within one transaction.
    let tx = TestTransactionBuilder::new(alice.address, alice.gas, rgp)
        .transfer_funds_to_address_balance(
            FundSource::address_fund(),
            vec![(1000, carol)],
            sui_type(),
        )
        .build();
    let e1 = env.execute(tx, &mut alice).await;
    let tx = TestTransactionBuilder::new(bob.address, bob.gas, rgp)
        .transfer_funds_to_address_balance(
            FundSource::address_fund(),
            vec![(500, dave), (700, bob.address)],
            sui_type(),
        )
        .build();
    let e2 = env.execute(tx, &mut bob).await;
    assert_eq!(env.settle_and_compare(&[e1, e2]).await, 3);
    assert!(!env.accumulator_exists(alice.address));
    assert_eq!(env.balance(bob.address), 1500);
    assert_eq!(env.balance(carol), 4000);
    assert_eq!(env.balance(dave), 4500);

    // Checkpoint 3: alice's accumulator is created again.
    let tx = TestTransactionBuilder::new(bob.address, bob.gas, rgp)
        .transfer_funds_to_address_balance(
            FundSource::address_fund(),
            vec![(1500, alice.address)],
            sui_type(),
        )
        .build();
    let e1 = env.execute(tx, &mut bob).await;
    assert_eq!(env.settle_and_compare(&[e1]).await, 2);
    assert!(!env.accumulator_exists(bob.address));
    assert_eq!(env.balance(alice.address), 1500);

    // The accumulator root version advances identically.
    let root_version = |authority: &AuthorityState| {
        authority
            .get_object_store()
            .get_object(&SUI_ACCUMULATOR_ROOT_OBJECT_ID)
            .unwrap()
            .version()
    };
    assert_eq!(
        root_version(&env.move_authority),
        root_version(&env.native_authority)
    );
}
