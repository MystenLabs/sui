// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use fastcrypto::groups::bls12381::{G1Element, G2Element, Scalar};
use fastcrypto::groups::{GroupElement, HashToGroupElement, Scalar as _};
use move_core_types::ident_str;
use sui_protocol_config::ProtocolConfig;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::accumulator_root::AccumulatorValue;
use sui_types::base_types::{ObjectID, ObjectRef};
use sui_types::crypto::{AccountKeyPair, deterministic_random_account_key, get_key_pair};
use sui_types::digests::TransactionDigest;
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::gas_coin::GAS;
use sui_types::messages_consensus::ConsensusTransaction;
use sui_types::object::Object;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{
    ObjectArg, ProgrammableTransaction, SharedObjectMutability, Transaction, TransactionData,
};
use sui_types::utils::to_sender_signed_transaction;

use super::*;
use crate::authority::AuthorityState;
use crate::authority::shared_object_version_manager::Schedulable;
use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::consensus_adapter::consensus_tests::test_user_transaction;
use crate::consensus_test_utils::{
    CapturedTransactions, TestConsensusCommit, setup_consensus_handler_for_testing,
};

const FEE: u64 = 1_000_000;

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
impl EncryptedTransactionKeys for TestKeys {
    fn request(&self, _epoch: EpochId, id_message: Vec<u8>) {
        self.requested.lock().push(id_message);
    }

    async fn wait(&self, _epoch: EpochId, id_message: Vec<u8>) -> EncryptedTransactionKey {
        G1Element::hash_to_group_element(&id_message) * self.master
    }

    fn beacon_public_key(&self, _epoch: EpochId) -> Option<BeaconPublicKey> {
        Some(self.public_key())
    }
}

fn enable_encrypted_transactions(max_per_commit: u64) -> sui_protocol_config::OverrideGuard {
    ProtocolConfig::apply_overrides_for_testing(move |_, mut config| {
        // Without the random beacon the test authority has no randomness manager, so the test
        // key provider alone decides whether the beacon key is available.
        config.set_random_beacon_for_testing(false);
        config.set_encrypted_transactions_for_testing(true);
        config.set_encrypted_transaction_max_per_commit_for_testing(max_per_commit);
        config.set_encrypted_transaction_fee_for_testing(FEE);
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
    let gas_objects: Vec<_> = (0..6)
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
        .set_encrypted_transaction_keys(keys.clone());
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
    /// A programmable transaction that reads the shared coin object.
    fn read_shared_object(&self) -> ProgrammableTransaction {
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
        builder.finish()
    }

    /// A programmable transaction with an owned input, which the vote does not lock.
    fn use_owned_object(&self, object: &Object) -> ProgrammableTransaction {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder
            .transfer_object(self.sender, object.compute_full_object_reference())
            .unwrap();
        builder.finish()
    }

    /// A programmable transaction that declares withdrawals and transfers gas.
    fn withdraw(&self, withdrawals: Vec<FundsWithdrawalArg>) -> ProgrammableTransaction {
        let mut builder = ProgrammableTransactionBuilder::new();
        for withdrawal in withdrawals {
            builder.funds_withdrawal(withdrawal).unwrap();
        }
        builder.transfer_sui(self.sender, Some(1));
        builder.finish()
    }

    fn gas(&self, index: usize) -> ObjectRef {
        self.gas_objects[index].compute_object_reference()
    }

    /// Wraps `encrypted` into a transaction from `sender` that pays gas with `gas`.
    fn envelope(
        &self,
        encrypted: EncryptedTransaction,
        sender: SuiAddress,
        keypair: &AccountKeyPair,
        gas: ObjectRef,
    ) -> Transaction {
        let data = TransactionData::new_with_gas_coins(
            TransactionKind::EncryptedTransaction(encrypted),
            sender,
            vec![gas],
            100_000_000,
            FEE / sui_types::encrypted_transaction::MIN_COMPUTATION_UNITS,
        );
        to_sender_signed_transaction(data, keypair)
    }

    fn encrypt(&self, transaction: &ProgrammableTransaction) -> EncryptedTransaction {
        EncryptedTransaction::seal(
            &mut rand::thread_rng(),
            &self.keys.public_key(),
            self.state.epoch_store_for_testing().epoch(),
            self.sender,
            transaction,
        )
    }

    fn encrypted_transaction(
        &self,
        transaction: &ProgrammableTransaction,
        gas_index: usize,
    ) -> Transaction {
        self.envelope(
            self.encrypt(transaction),
            self.sender,
            &self.keypair,
            self.gas(gas_index),
        )
    }

    /// A plaintext user transaction that pays gas with an owned coin.
    async fn plain_transaction(&self, gas_index: usize) -> ConsensusTransaction {
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

    /// Verifies the signature of `transaction` and wraps it in a consensus message.
    fn message(&self, transaction: Transaction) -> ConsensusTransaction {
        let verified = self
            .state
            .epoch_store_for_testing()
            .verify_transaction_with_current_aliases(transaction)
            .unwrap();
        ConsensusTransaction::new_user_transaction_v2_message(&self.state.name, verified.into())
    }
}

fn user_digest(message: &ConsensusTransaction) -> TransactionDigest {
    *message.kind.as_user_transaction().unwrap().digest()
}

/// Returns the user transactions that the handler sent to execution, per commit.
async fn scheduled(
    captured: &CapturedTransactions,
    expected_commits: usize,
) -> Vec<Vec<VerifiedExecutableTransaction>> {
    for _ in 0..200 {
        if captured.lock().len() >= expected_commits {
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
                        Some(tx.clone())
                    }
                    _ => None,
                })
                .collect()
        })
        .collect()
}

fn digests(transactions: &[VerifiedExecutableTransaction]) -> Vec<TransactionDigest> {
    transactions.iter().map(|tx| *tx.digest()).collect()
}

fn decrypted(transaction: &VerifiedExecutableTransaction) -> &ProgrammableTransaction {
    let TransactionKind::EncryptedTransaction(et) = transaction.transaction_data().kind() else {
        panic!("{} is not an encrypted transaction", transaction.digest());
    };
    et.decrypted()
        .expect("scheduled encrypted transaction is decrypted")
}

#[tokio::test]
async fn encrypted_transaction_is_scheduled_before_the_next_commit() {
    let _guard = enable_encrypted_transactions(2);
    let fixture = fixture().await;

    let inner = fixture.read_shared_object();
    let encrypted = fixture.encrypted_transaction(&inner, 0);
    let same_commit = fixture.plain_transaction(1).await;
    let next_commit = fixture.plain_transaction(2).await;
    let TransactionKind::EncryptedTransaction(et) = encrypted.transaction_data().kind() else {
        unreachable!()
    };
    let id_message = et.id_message(&fixture.sender);

    let mut setup = setup_consensus_handler_for_testing(&fixture.state).await;
    setup
        .consensus_handler
        .handle_consensus_commit_for_test(TestConsensusCommit::new(
            vec![fixture.message(encrypted.clone()), same_commit.clone()],
            10,
            1_000,
            1,
        ))
        .await;
    assert_eq!(
        *fixture.keys.requested.lock(),
        vec![id_message],
        "the key is requested once the transaction's commit is processed"
    );
    assert!(setup.consensus_handler.encrypted_transactions.has_pending());

    setup
        .consensus_handler
        .handle_consensus_commit_for_test(TestConsensusCommit::new(
            vec![next_commit.clone()],
            11,
            2_000,
            2,
        ))
        .await;

    let commits = scheduled(&setup.captured_transactions, 2).await;
    assert_eq!(digests(&commits[0]), vec![user_digest(&same_commit)]);
    assert_eq!(
        digests(&commits[1]),
        vec![*encrypted.digest(), user_digest(&next_commit)]
    );
    assert_eq!(decrypted(&commits[1][0]), &inner);
    assert_eq!(
        commits[1][0]
            .transaction_data()
            .kind()
            .shared_input_objects()
            .count(),
        1,
        "the scheduled transaction exposes the decrypted shared input"
    );
}

#[tokio::test]
async fn invalid_decrypted_transaction_runs_no_command() {
    let _guard = enable_encrypted_transactions(5);
    let fixture = fixture().await;

    let sender_withdrawal =
        |amount| FundsWithdrawalArg::balance_from_sender(amount, GAS::type_tag());
    let allowance_withdrawal = FundsWithdrawalArg::balance_from_allowance(
        1,
        GAS::type_tag(),
        fixture.sender,
        ObjectID::random(),
    );
    let invalid = [
        fixture.use_owned_object(&fixture.gas_objects[5]),
        fixture.withdraw(vec![allowance_withdrawal]),
        fixture.withdraw(vec![sender_withdrawal(0)]),
        // The sender's address balance is empty.
        fixture.withdraw(vec![sender_withdrawal(1)]),
        // The sum of the reservations overflows.
        fixture.withdraw(vec![
            sender_withdrawal(u64::MAX),
            sender_withdrawal(u64::MAX - 1),
        ]),
    ];
    let envelopes: Vec<_> = invalid
        .iter()
        .enumerate()
        .map(|(index, transaction)| fixture.encrypted_transaction(transaction, index))
        .collect();
    let messages = envelopes
        .iter()
        .map(|envelope| fixture.message(envelope.clone()))
        .collect();

    let mut setup = setup_consensus_handler_for_testing(&fixture.state).await;
    for (round, transactions) in [(10, messages), (11, vec![])] {
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

    let commits = scheduled(&setup.captured_transactions, 2).await;
    assert!(commits[0].is_empty());
    assert_eq!(commits[1].len(), envelopes.len());
    for transaction in &commits[1] {
        assert!(decrypted(transaction).commands.is_empty());
        assert!(!transaction.transaction_data().has_funds_withdrawals());
    }
}

#[tokio::test]
async fn copied_ciphertext_does_not_decrypt() {
    let _guard = enable_encrypted_transactions(2);
    let fixture = fixture().await;

    // An attacker copies the sender's ciphertext into its own transaction. The released key
    // belongs to the attacker's identity, so the ciphertext does not decrypt.
    let (attacker, attacker_keypair): (_, AccountKeyPair) = get_key_pair();
    let attacker_gas = Object::with_id_owner_for_testing(ObjectID::random(), attacker);
    fixture
        .state
        .insert_objects_unsafe_for_testing_only(std::slice::from_ref(&attacker_gas))
        .await
        .unwrap();
    let copied = fixture.envelope(
        fixture.encrypt(&fixture.read_shared_object()),
        attacker,
        &attacker_keypair,
        attacker_gas.compute_object_reference(),
    );

    let mut setup = setup_consensus_handler_for_testing(&fixture.state).await;
    for (round, transactions) in [(10, vec![fixture.message(copied.clone())]), (11, vec![])] {
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

    let commits = scheduled(&setup.captured_transactions, 2).await;
    assert_eq!(digests(&commits[1]), vec![*copied.digest()]);
    assert!(decrypted(&commits[1][0]).commands.is_empty());
}

#[tokio::test]
async fn transactions_over_the_commit_limit_run_no_command() {
    let _guard = enable_encrypted_transactions(1);
    let fixture = fixture().await;

    let inner = fixture.read_shared_object();
    let first = fixture.encrypted_transaction(&inner, 0);
    let second = fixture.encrypted_transaction(&inner, 1);

    let mut setup = setup_consensus_handler_for_testing(&fixture.state).await;
    for (round, transactions) in [
        (
            10,
            vec![
                fixture.message(first.clone()),
                fixture.message(second.clone()),
            ],
        ),
        (11, vec![]),
    ] {
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
    // The resolve step requests the key again, which has no effect.
    let requested: std::collections::HashSet<_> =
        fixture.keys.requested.lock().iter().cloned().collect();
    assert_eq!(
        requested.len(),
        1,
        "no key is released for the transaction over the limit"
    );

    let commits = scheduled(&setup.captured_transactions, 2).await;
    // The transaction over the limit runs no command in its own commit.
    assert_eq!(digests(&commits[0]), vec![*second.digest()]);
    assert!(decrypted(&commits[0][0]).commands.is_empty());
    assert_eq!(digests(&commits[1]), vec![*first.digest()]);
    assert_eq!(decrypted(&commits[1][0]), &inner);
}

#[tokio::test]
async fn checkpoint_transactions_are_decrypted_from_prologue_keys() {
    let _guard = enable_encrypted_transactions(2);
    let fixture = fixture().await;
    let inner = fixture.read_shared_object();
    let encrypted = fixture.encrypted_transaction(&inner, 0);
    let TransactionKind::EncryptedTransaction(et) = encrypted.transaction_data().kind() else {
        unreachable!()
    };
    let key = fixture.keys.wait(0, et.id_message(&fixture.sender)).await;

    let executable = |tx: Transaction| {
        VerifiedExecutableTransaction::new_from_checkpoint(
            sui_types::transaction::VerifiedTransaction::new_unchecked(tx),
            0,
            1,
        )
    };
    let prologue = |round, encrypted_transactions| {
        Arc::new(
            sui_types::transaction::VerifiedTransaction::new_consensus_commit_prologue_v4(
                0,
                round,
                0,
                Default::default(),
                sui_types::messages_consensus::ConsensusDeterminedVersionAssignments::CancelledAndEncryptedTransactions {
                    cancelled_transactions: vec![],
                    encrypted_transactions,
                },
                sui_types::digests::AdditionalConsensusStateDigest::ZERO,
            ),
        )
    };
    let with_key = prologue(
        1,
        vec![EncryptedTransactionResolution {
            transaction_digest: *encrypted.digest(),
            key: key_to_bytes(&key),
            executes: true,
        }],
    );
    let as_executable = |tx: &Arc<sui_types::transaction::VerifiedTransaction>| {
        VerifiedExecutableTransaction::new_system((**tx).clone(), 0)
    };

    // The prologue is in the same checkpoint. Earlier checkpoints are not read.
    let transactions = decrypt_checkpoint_transactions(
        vec![as_executable(&with_key), executable(encrypted.clone())],
        std::iter::from_fn(|| -> Option<Vec<Arc<VerifiedTransaction>>> {
            panic!("earlier checkpoints must not be read")
        }),
    );
    assert_eq!(decrypted(&transactions[1]), &inner);

    // The commit spans two checkpoints, so the prologue is in the previous checkpoint.
    let transactions = decrypt_checkpoint_transactions(
        vec![executable(encrypted.clone())],
        vec![vec![with_key.clone()]],
    );
    assert_eq!(decrypted(&transactions[0]), &inner);

    // The search stops at the latest checkpoint with a prologue. A key recorded in an older
    // prologue belongs to another commit and is not used.
    let transactions = decrypt_checkpoint_transactions(
        vec![executable(encrypted.clone())],
        vec![vec![prologue(2, vec![])], vec![with_key]],
    );
    assert!(needs_key(&transactions[0]));
}

#[tokio::test]
async fn withdrawals_of_one_commit_share_the_balance() {
    let _guard = enable_encrypted_transactions(3);
    let fixture = fixture().await;
    let balance = AccumulatorValue::create_for_testing(
        fixture.sender,
        Balance::type_tag(GAS::type_tag()),
        100,
    );
    fixture
        .state
        .insert_objects_unsafe_for_testing_only(&[balance])
        .await
        .unwrap();

    let withdraw = |amount| {
        fixture.withdraw(vec![FundsWithdrawalArg::balance_from_sender(
            amount,
            GAS::type_tag(),
        )])
    };
    // The second withdrawal exceeds what the first leaves. It reserves nothing, so the third
    // is covered.
    let amounts = [60, 50, 40];
    let envelopes: Vec<_> = amounts
        .iter()
        .enumerate()
        .map(|(index, amount)| fixture.encrypted_transaction(&withdraw(*amount), index))
        .collect();
    let messages = envelopes
        .iter()
        .map(|envelope| fixture.message(envelope.clone()))
        .collect();

    let mut setup = setup_consensus_handler_for_testing(&fixture.state).await;
    for (round, transactions) in [(10, messages), (11, vec![])] {
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

    let commits = scheduled(&setup.captured_transactions, 2).await;
    assert_eq!(
        digests(&commits[1]),
        envelopes
            .iter()
            .map(|envelope| *envelope.digest())
            .collect::<Vec<_>>()
    );
    let executed: Vec<_> = commits[1]
        .iter()
        .map(|tx| !decrypted(tx).commands.is_empty())
        .collect();
    assert_eq!(executed, vec![true, false, true]);
}
