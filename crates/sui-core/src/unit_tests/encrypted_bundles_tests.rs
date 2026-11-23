// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use fastcrypto::groups::bls12381::{G1Element, G2Element, Scalar};
use fastcrypto::groups::{GroupElement, HashToGroupElement, Scalar as _};
use move_core_types::ident_str;
use sui_protocol_config::ProtocolConfig;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::crypto::{AccountKeyPair, deterministic_random_account_key, get_key_pair};
use sui_types::encrypted_bundle::BundlePlaintext;
use sui_types::gas_coin::GAS;
use sui_types::messages_consensus::ConsensusTransaction;
use sui_types::object::Object;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{ObjectArg, SharedObjectMutability, TransactionData};

use sui_types::utils::to_sender_signed_transaction;

use super::*;
use crate::authority::AuthorityState;
use crate::authority::shared_object_version_manager::Schedulable;
use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::consensus_adapter::consensus_tests::test_user_transaction;
use crate::consensus_test_utils::{
    CapturedTransactions, TestConsensusCommit, setup_consensus_handler_for_testing,
};

/// Derives keys directly from a master key instead of from beacon key shares.
struct TestKeys {
    master: Scalar,
    requested: parking_lot::Mutex<Vec<Vec<u8>>>,
}

impl TestKeys {
    fn new() -> Self {
        Self {
            master: Scalar::rand(&mut rand::thread_rng()),
            requested: parking_lot::Mutex::new(vec![]),
        }
    }

    fn public_key(&self) -> BeaconPublicKey {
        G2Element::generator() * self.master
    }
}

#[async_trait]
impl EncryptedBundleKeys for TestKeys {
    fn request(&self, _epoch: EpochId, id_message: Vec<u8>) {
        self.requested.lock().push(id_message);
    }

    async fn wait(&self, _epoch: EpochId, id_message: Vec<u8>) -> BundleKey {
        G1Element::hash_to_group_element(&id_message) * self.master
    }

    fn beacon_public_key(&self, _epoch: EpochId) -> Option<BeaconPublicKey> {
        Some(self.public_key())
    }
}

fn enable_bundles(fee: u64) -> sui_protocol_config::OverrideGuard {
    ProtocolConfig::apply_overrides_for_testing(move |_, mut config| {
        // Without the random beacon the test authority has no randomness manager, so the test
        // key provider alone decides whether the beacon key is available.
        config.set_random_beacon_for_testing(false);
        config.set_encrypted_bundles_for_testing(true);
        config.set_encrypted_bundle_max_per_commit_for_testing(2);
        config.set_encrypted_bundle_max_transactions_for_testing(10);
        config.set_encrypted_bundle_fee_for_testing(fee);
        config
    })
}

struct Fixture {
    state: Arc<AuthorityState>,
    keys: Arc<TestKeys>,
    sender: SuiAddress,
    keypair: AccountKeyPair,
    gas_objects: Vec<Object>,
    shared_object: Object,
}

async fn fixture() -> Fixture {
    telemetry_subscribers::init_for_testing();
    let (sender, keypair) = deterministic_random_account_key();
    let gas_objects: Vec<_> = (0..3)
        .map(|_| Object::with_id_owner_for_testing(ObjectID::random(), sender))
        .collect();
    let shared_object = Object::shared_for_testing();
    let mut starting_objects = gas_objects.clone();
    starting_objects.push(shared_object.clone());
    let state = TestAuthorityBuilder::new()
        .with_starting_objects(&starting_objects)
        .build()
        .await;
    let keys = Arc::new(TestKeys::new());
    state
        .epoch_store_for_testing()
        .set_encrypted_bundle_keys(keys.clone());
    Fixture {
        state,
        keys,
        sender,
        keypair,
        gas_objects,
        shared_object,
    }
}

impl Fixture {
    /// A transaction that reads the shared object and pays gas from the address balance.
    fn bundle_transaction(&self, nonce: u32) -> Transaction {
        self.bundle_transaction_signed_by(nonce, &self.keypair)
    }

    fn bundle_transaction_signed_by(&self, nonce: u32, keypair: &AccountKeyPair) -> Transaction {
        let epoch_store = self.state.epoch_store_for_testing();
        let mut builder = ProgrammableTransactionBuilder::new();
        let coin = builder
            .obj(ObjectArg::SharedObject {
                id: self.shared_object.id(),
                initial_shared_version: self.shared_object.version(),
                mutability: SharedObjectMutability::Immutable,
            })
            .unwrap();
        builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            ident_str!("coin").to_owned(),
            ident_str!("value").to_owned(),
            vec![GAS::type_tag()],
            vec![coin],
        );
        let rgp = epoch_store.reference_gas_price();
        let data = TransactionData::new_programmable_with_address_balance_gas(
            self.sender,
            builder.finish(),
            rgp * 10_000,
            rgp,
            epoch_store.get_chain_identifier(),
            epoch_store.epoch(),
            nonce,
        );
        to_sender_signed_transaction(data, keypair)
    }

    /// A plaintext user transaction that pays gas with an owned coin.
    async fn owned_transaction(&self, gas_index: usize) -> ConsensusTransaction {
        let transaction = test_user_transaction(
            &self.state,
            self.sender,
            &self.keypair,
            self.gas_objects[gas_index].clone(),
            vec![],
        )
        .await;
        ConsensusTransaction::new_user_transaction_v2_message(&self.state.name, transaction.into())
    }

    fn bundle_message(
        &self,
        transactions: Vec<Transaction>,
        fee_tx_digest: TransactionDigest,
    ) -> (EncryptedBundle, ConsensusTransaction) {
        let bundle = EncryptedBundle::seal(
            &mut rand::thread_rng(),
            &self.keys.public_key(),
            self.state.epoch_store_for_testing().epoch(),
            self.state.name,
            &BundlePlaintext { transactions },
        );
        let message = ConsensusTransaction::new_encrypted_bundle(EncryptedBundleMessage {
            bundle: bundle.clone(),
            fee_tx_digest,
        });
        (bundle, message)
    }
}

fn user_digest(message: &ConsensusTransaction) -> TransactionDigest {
    *message.kind.as_user_transaction().unwrap().digest()
}

/// Returns the user transaction digests that the handler sent to execution, per message.
async fn scheduled_digests(
    captured: &CapturedTransactions,
    expected_messages: usize,
) -> Vec<Vec<TransactionDigest>> {
    for _ in 0..200 {
        if captured.lock().len() >= expected_messages {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    captured
        .lock()
        .iter()
        .map(|(schedulables, _)| {
            schedulables
                .iter()
                .filter_map(|(schedulable, _)| match schedulable {
                    Schedulable::Transaction(tx) if !tx.transaction_data().is_system_tx() => {
                        Some(*tx.digest())
                    }
                    _ => None,
                })
                .collect()
        })
        .collect()
}

#[tokio::test]
async fn bundle_transactions_are_scheduled_before_the_next_commit() {
    let _guard = enable_bundles(1_000);
    let fixture = fixture().await;

    let fee_tx = fixture.owned_transaction(0).await;
    let first = fixture.bundle_transaction(1);
    let second = fixture.bundle_transaction(2);
    let (bundle, bundle_message) =
        fixture.bundle_message(vec![first.clone(), second.clone()], user_digest(&fee_tx));
    let next_commit_tx = fixture.owned_transaction(1).await;

    let mut setup = setup_consensus_handler_for_testing(&fixture.state).await;
    setup
        .consensus_handler
        .handle_consensus_commit_for_test(TestConsensusCommit::new(
            vec![fee_tx.clone(), bundle_message],
            10,
            1_000,
            1,
        ))
        .await;
    assert_eq!(
        *fixture.keys.requested.lock(),
        vec![bundle.id_message()],
        "the key is requested once the bundle's commit is processed"
    );

    setup
        .consensus_handler
        .handle_consensus_commit_for_test(TestConsensusCommit::new(
            vec![next_commit_tx.clone()],
            11,
            2_000,
            2,
        ))
        .await;

    let scheduled: Vec<_> = scheduled_digests(&setup.captured_transactions, 2)
        .await
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(
        scheduled,
        vec![
            user_digest(&fee_tx),
            *first.digest(),
            *second.digest(),
            user_digest(&next_commit_tx),
        ]
    );
}

#[tokio::test]
async fn bundle_without_fee_payment_is_dropped() {
    let _guard = enable_bundles(1_000);
    let fixture = fixture().await;

    let unsequenced_fee_tx = fixture.owned_transaction(0).await;
    let (_, bundle_message) = fixture.bundle_message(
        vec![fixture.bundle_transaction(1)],
        user_digest(&unsequenced_fee_tx),
    );

    let mut setup = setup_consensus_handler_for_testing(&fixture.state).await;
    setup
        .consensus_handler
        .handle_consensus_commit_for_test(TestConsensusCommit::new(
            vec![bundle_message],
            10,
            1_000,
            1,
        ))
        .await;
    assert!(fixture.keys.requested.lock().is_empty());
    assert!(!setup.consensus_handler.encrypted_bundles.has_pending());
}

#[tokio::test]
async fn bundle_with_low_fee_is_dropped() {
    // The fee transaction's gas price times the smallest computation charge is below this fee.
    let _guard = enable_bundles(u64::MAX);
    let fixture = fixture().await;

    let fee_tx = fixture.owned_transaction(0).await;
    let (_, bundle_message) =
        fixture.bundle_message(vec![fixture.bundle_transaction(1)], user_digest(&fee_tx));

    let mut setup = setup_consensus_handler_for_testing(&fixture.state).await;
    setup
        .consensus_handler
        .handle_consensus_commit_for_test(TestConsensusCommit::new(
            vec![fee_tx, bundle_message],
            10,
            1_000,
            1,
        ))
        .await;
    assert!(fixture.keys.requested.lock().is_empty());
}

#[tokio::test]
async fn invalid_bundle_transactions_are_dropped() {
    let _guard = enable_bundles(1_000);
    let fixture = fixture().await;

    let fee_tx = fixture.owned_transaction(0).await;
    let valid = fixture.bundle_transaction(1);
    // Pays gas with an owned coin, which decrypted transactions may not do.
    let owned = fixture
        .owned_transaction(1)
        .await
        .kind
        .into_user_transaction()
        .unwrap();
    // Signed by a key that does not belong to the sender.
    let (_, other_keypair): (_, AccountKeyPair) = get_key_pair();
    let forged = fixture.bundle_transaction_signed_by(2, &other_keypair);

    let (_, bundle_message) = fixture.bundle_message(
        vec![owned.clone(), valid.clone(), forged],
        user_digest(&fee_tx),
    );

    let mut setup = setup_consensus_handler_for_testing(&fixture.state).await;
    for (round, transactions) in [(10, vec![fee_tx.clone(), bundle_message]), (11, vec![])] {
        setup
            .consensus_handler
            .handle_consensus_commit_for_test(TestConsensusCommit::new(
                transactions,
                round,
                round * 100,
                round - 9,
            ))
            .await;
    }

    let scheduled: Vec<_> = scheduled_digests(&setup.captured_transactions, 2)
        .await
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(scheduled, vec![user_digest(&fee_tx), *valid.digest()]);
}
