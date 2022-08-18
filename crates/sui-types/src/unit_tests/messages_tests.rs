// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use narwhal_crypto::traits::AggregateAuthenticator;
use narwhal_crypto::traits::KeyPair;
use roaring::RoaringBitmap;

use crate::crypto::bcs_signable_test::{get_obligation_input, Foo};
use crate::crypto::Secp256k1SuiSignature;
use crate::crypto::SuiKeyPair;
use crate::crypto::{get_key_pair, AccountKeyPair, AuthorityKeyPair, AuthorityPublicKeyBytes};
use crate::messages_checkpoint::CheckpointContents;
use crate::messages_checkpoint::CheckpointSummary;
use crate::object::Owner;

use super::*;
fn random_object_ref() -> ObjectRef {
    (
        ObjectID::random(),
        SequenceNumber::new(),
        ObjectDigest::new([0; 32]),
    )
}

#[test]
fn test_signed_values() {
    let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();
    // TODO: refactor this test to not reuse the same keys for user and authority signing
    let (_a1, sec1): (_, AuthorityKeyPair) = get_key_pair();
    let (_a2, sec2): (_, AuthorityKeyPair) = get_key_pair();
    let (_a3, sec3): (_, AuthorityKeyPair) = get_key_pair();
    let (a_sender, sender_sec): (_, AccountKeyPair) = get_key_pair();
    let (_a_sender2, sender_sec2): (_, AccountKeyPair) = get_key_pair();

    authorities.insert(
        /* address */ AuthorityPublicKeyBytes::from(sec1.public()),
        /* voting right */ 1,
    );
    authorities.insert(
        /* address */ AuthorityPublicKeyBytes::from(sec2.public()),
        /* voting right */ 0,
    );
    let committee = Committee::new(0, authorities).unwrap();

    let transaction = Transaction::from_data(
        TransactionData::new_transfer(
            _a2,
            random_object_ref(),
            a_sender,
            random_object_ref(),
            10000,
        ),
        &sender_sec,
    );
    let bad_transaction = Transaction::from_data(
        TransactionData::new_transfer(
            _a2,
            random_object_ref(),
            a_sender,
            random_object_ref(),
            10000,
        ),
        &sender_sec2,
    );

    let v = SignedTransaction::new(
        committee.epoch(),
        transaction.data().clone(),
        &sec1,
        AuthorityPublicKeyBytes::from(sec1.public()),
    );
    assert!(v.verify(&committee).is_ok());

    let v = SignedTransaction::new(
        committee.epoch(),
        transaction.data().clone(),
        &sec2,
        AuthorityPublicKeyBytes::from(sec2.public()),
    );
    assert!(v.verify(&committee).is_err());

    let v = SignedTransaction::new(
        committee.epoch(),
        transaction.into_data(),
        &sec3,
        AuthorityPublicKeyBytes::from(sec3.public()),
    );
    assert!(v.verify(&committee).is_err());

    let v = SignedTransaction::new(
        committee.epoch(),
        bad_transaction.into_data(),
        &sec1,
        AuthorityPublicKeyBytes::from(sec1.public()),
    );
    assert!(v.verify(&committee).is_err());
}

#[test]
fn test_certificates() {
    let (_a1, sec1): (_, AuthorityKeyPair) = get_key_pair();
    let (a2, sec2): (_, AuthorityKeyPair) = get_key_pair();
    let (_a3, sec3): (_, AuthorityKeyPair) = get_key_pair();
    let (a_sender, sender_sec): (_, AccountKeyPair) = get_key_pair();
    let (_a_sender2, sender_sec2): (_, AccountKeyPair) = get_key_pair();

    let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();
    authorities.insert(
        /* address */ AuthorityPublicKeyBytes::from(sec1.public()),
        /* voting right */ 1,
    );
    authorities.insert(
        /* address */ AuthorityPublicKeyBytes::from(sec2.public()),
        /* voting right */ 1,
    );
    let committee = Committee::new(0, authorities).unwrap();

    let transaction = Transaction::from_data(
        TransactionData::new_transfer(
            a2,
            random_object_ref(),
            a_sender,
            random_object_ref(),
            10000,
        ),
        &sender_sec,
    );
    let bad_transaction = Transaction::from_data(
        TransactionData::new_transfer(
            a2,
            random_object_ref(),
            a_sender,
            random_object_ref(),
            10000,
        ),
        &sender_sec2,
    );

    let v1 = SignedTransaction::new(
        committee.epoch(),
        transaction.data().clone(),
        &sec1,
        AuthorityPublicKeyBytes::from(sec1.public()),
    );
    let v2 = SignedTransaction::new(
        committee.epoch(),
        transaction.data().clone(),
        &sec2,
        AuthorityPublicKeyBytes::from(sec2.public()),
    );
    let v3 = SignedTransaction::new(
        committee.epoch(),
        transaction.data().clone(),
        &sec3,
        AuthorityPublicKeyBytes::from(sec3.public()),
    );

    let mut builder = SignatureAggregator::try_new(transaction.clone(), &committee).unwrap();
    assert!(builder
        .append(
            v1.auth_signature.authority,
            v1.auth_signature.signature.clone()
        )
        .unwrap()
        .is_none());
    let c = builder
        .append(v2.auth_signature.authority, v2.auth_signature.signature)
        .unwrap()
        .unwrap();

    assert!(c.verify(&committee).is_ok());

    let mut builder = SignatureAggregator::try_new(transaction, &committee).unwrap();
    assert!(builder
        .append(v1.auth_signature.authority, v1.auth_signature.signature)
        .unwrap()
        .is_none());
    assert!(builder
        .append(v3.auth_signature.authority, v3.auth_signature.signature)
        .is_err());

    assert!(SignatureAggregator::try_new(bad_transaction, &committee).is_err());
}

#[test]
fn test_new_with_signatures() {
    let message: messages_tests::Foo = Foo("some data".to_string());
    let mut signatures: Vec<(AuthorityName, AuthoritySignature)> = Vec::new();
    let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();

    for _ in 0..5 {
        let (_, sec): (_, AuthorityKeyPair) = get_key_pair();
        let sig = AuthoritySignature::new(&message, &sec);
        signatures.push((AuthorityPublicKeyBytes::from(sec.public()), sig));
        authorities.insert(AuthorityPublicKeyBytes::from(sec.public()), 1);
    }
    let (_, sec): (_, AuthorityKeyPair) = get_key_pair();
    authorities.insert(AuthorityPublicKeyBytes::from(sec.public()), 1);

    let committee = Committee::new(0, authorities.clone()).unwrap();
    let quorum =
        AuthorityStrongQuorumSignInfo::new_with_signatures(signatures.clone(), &committee).unwrap();

    let sig_clone = signatures.clone();
    let mut alphabetical_authorities = sig_clone
        .iter()
        .map(|(pubx, _)| pubx)
        .collect::<Vec<&AuthorityName>>();
    alphabetical_authorities.sort();
    assert_eq!(
        quorum
            .authorities(&committee)
            .collect::<SuiResult<Vec<&AuthorityName>>>()
            .unwrap(),
        alphabetical_authorities
    );

    let (mut obligation, idx) = get_obligation_input(&message);
    assert!(quorum
        .add_to_verification_obligation(&committee, &mut obligation, idx)
        .is_ok());
    assert!(obligation.verify_all().is_ok());
}

#[test]
fn test_handle_reject_malicious_signature() {
    let message: messages_tests::Foo = Foo("some data".to_string());
    let mut signatures: Vec<(AuthorityName, AuthoritySignature)> = Vec::new();
    let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();

    for i in 0..5 {
        let (_, sec): (_, AuthorityKeyPair) = get_key_pair();
        let sig = AuthoritySignature::new(&Foo("some data".to_string()), &sec);
        authorities.insert(AuthorityPublicKeyBytes::from(sec.public()), 1);
        if i < 4 {
            signatures.push((AuthorityPublicKeyBytes::from(sec.public()), sig))
        };
    }

    let committee = Committee::new(0, authorities.clone()).unwrap();
    let mut quorum =
        AuthorityStrongQuorumSignInfo::new_with_signatures(signatures, &committee).unwrap();
    {
        let (_, sec): (_, AuthorityKeyPair) = get_key_pair();
        let sig = AuthoritySignature::new(&message, &sec);
        quorum.signature.add_signature(sig).unwrap();
    }
    let (mut obligation, idx) = get_obligation_input(&message);
    assert!(quorum
        .add_to_verification_obligation(&committee, &mut obligation, idx)
        .is_ok());
    assert!(obligation.verify_all().is_err());
}

#[test]
fn test_bitmap_out_of_range() {
    let message: messages_tests::Foo = Foo("some data".to_string());
    let mut signatures: Vec<(AuthorityName, AuthoritySignature)> = Vec::new();
    let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();
    for _ in 0..5 {
        let (_, sec): (_, AuthorityKeyPair) = get_key_pair();
        let sig = AuthoritySignature::new(&Foo("some data".to_string()), &sec);
        authorities.insert(AuthorityPublicKeyBytes::from(sec.public()), 1);
        signatures.push((AuthorityPublicKeyBytes::from(sec.public()), sig));
    }

    let committee = Committee::new(0, authorities.clone()).unwrap();
    let mut quorum =
        AuthorityStrongQuorumSignInfo::new_with_signatures(signatures, &committee).unwrap();

    // Insert outside of range
    quorum.signers_map.insert(10);

    let (mut obligation, idx) = get_obligation_input(&message);
    assert!(quorum
        .add_to_verification_obligation(&committee, &mut obligation, idx)
        .is_err());
}

#[test]
fn test_reject_extra_public_key() {
    let message: messages_tests::Foo = Foo("some data".to_string());
    let mut signatures: Vec<(AuthorityName, AuthoritySignature)> = Vec::new();
    let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();
    for _ in 0..5 {
        let (_, sec): (_, AuthorityKeyPair) = get_key_pair();
        let sig = AuthoritySignature::new(&Foo("some data".to_string()), &sec);
        authorities.insert(AuthorityPublicKeyBytes::from(sec.public()), 1);
        signatures.push((AuthorityPublicKeyBytes::from(sec.public()), sig));
    }

    signatures.sort_by_key(|k| k.0);

    let used_signatures: Vec<(AuthorityName, AuthoritySignature)> = vec![
        signatures[0].clone(),
        signatures[1].clone(),
        signatures[2].clone(),
        signatures[3].clone(),
    ];

    let committee = Committee::new(0, authorities.clone()).unwrap();
    let mut quorum =
        AuthorityStrongQuorumSignInfo::new_with_signatures(used_signatures, &committee).unwrap();

    quorum.signers_map.insert(3);

    let (mut obligation, idx) = get_obligation_input(&message);
    assert!(quorum
        .add_to_verification_obligation(&committee, &mut obligation, idx)
        .is_ok());
}

#[test]
fn test_reject_reuse_signatures() {
    let message: messages_tests::Foo = Foo("some data".to_string());
    let mut signatures: Vec<(AuthorityName, AuthoritySignature)> = Vec::new();
    let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();
    for _ in 0..5 {
        let (_, sec): (_, AuthorityKeyPair) = get_key_pair();
        let sig = AuthoritySignature::new(&Foo("some data".to_string()), &sec);
        authorities.insert(AuthorityPublicKeyBytes::from(sec.public()), 1);
        signatures.push((AuthorityPublicKeyBytes::from(sec.public()), sig));
    }

    let used_signatures: Vec<(AuthorityName, AuthoritySignature)> = vec![
        signatures[0].clone(),
        signatures[1].clone(),
        signatures[2].clone(),
        signatures[2].clone(),
    ];

    let committee = Committee::new(0, authorities.clone()).unwrap();
    let quorum =
        AuthorityStrongQuorumSignInfo::new_with_signatures(used_signatures, &committee).unwrap();

    let (mut obligation, idx) = get_obligation_input(&message);
    assert!(quorum
        .add_to_verification_obligation(&committee, &mut obligation, idx)
        .is_err());
}

#[test]
fn test_empty_bitmap() {
    let message: messages_tests::Foo = Foo("some data".to_string());
    let mut signatures: Vec<(AuthorityName, AuthoritySignature)> = Vec::new();
    let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();
    for _ in 0..5 {
        let (_, sec): (_, AuthorityKeyPair) = get_key_pair();
        let sig = AuthoritySignature::new(&Foo("some data".to_string()), &sec);
        authorities.insert(AuthorityPublicKeyBytes::from(sec.public()), 1);
        signatures.push((AuthorityPublicKeyBytes::from(sec.public()), sig));
    }

    let committee = Committee::new(0, authorities.clone()).unwrap();
    let mut quorum =
        AuthorityStrongQuorumSignInfo::new_with_signatures(signatures, &committee).unwrap();
    quorum.signers_map = RoaringBitmap::new();

    let (mut obligation, idx) = get_obligation_input(&message);
    assert!(quorum
        .add_to_verification_obligation(&committee, &mut obligation, idx)
        .is_err());
}

#[test]
fn test_digest_caching() {
    let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();
    // TODO: refactor this test to not reuse the same keys for user and authority signing
    let (a1, sec1): (_, AuthorityKeyPair) = get_key_pair();
    let (_a2, sec2): (_, AuthorityKeyPair) = get_key_pair();

    let (sa1, _ssec1): (_, AccountKeyPair) = get_key_pair();
    let (sa2, ssec2): (_, AccountKeyPair) = get_key_pair();

    authorities.insert(sec1.public().into(), 1);
    authorities.insert(sec2.public().into(), 0);

    let committee = Committee::new(0, authorities).unwrap();

    let transaction = Transaction::from_data(
        TransactionData::new_transfer(sa1, random_object_ref(), sa2, random_object_ref(), 10000),
        &ssec2,
    );

    let signed_tx = SignedTransaction::new(
        committee.epoch(),
        transaction.data().clone(),
        &sec1,
        AuthorityPublicKeyBytes::from(sec1.public()),
    );
    assert!(signed_tx.verify(&committee).is_ok());

    let initial_digest = *signed_tx.digest();

    // digest is cached
    assert_eq!(initial_digest, *signed_tx.digest());

    let serialized_tx = bincode::serialize(&signed_tx).unwrap();

    let deserialized_tx: SignedTransaction = bincode::deserialize(&serialized_tx).unwrap();

    // cached digest was not serialized/deserialized
    assert_ne!(initial_digest, *deserialized_tx.digest());

    let effects = TransactionEffects {
        status: ExecutionStatus::Success,
        gas_used: GasCostSummary {
            computation_cost: 0,
            storage_cost: 0,
            storage_rebate: 0,
        },
        shared_objects: Vec::new(),
        transaction_digest: initial_digest,
        created: Vec::new(),
        mutated: Vec::new(),
        unwrapped: Vec::new(),
        deleted: Vec::new(),
        wrapped: Vec::new(),
        gas_object: (random_object_ref(), Owner::AddressOwner(a1)),
        events: Vec::new(),
        dependencies: Vec::new(),
    };

    let signed_effects = SignedTransactionEffects::new(
        committee.epoch(),
        effects,
        &sec1,
        AuthorityPublicKeyBytes::from(sec1.public()),
    );

    let initial_effects_digest = *signed_effects.digest();

    // digest is cached
    assert_eq!(initial_effects_digest, *signed_effects.digest());

    let serialized_effects = bincode::serialize(&signed_effects).unwrap();

    let deserialized_effects: SignedTransactionEffects =
        bincode::deserialize(&serialized_effects).unwrap();

    // cached digest was not serialized/deserialized
    assert_ne!(initial_effects_digest, *deserialized_effects.digest());
}

#[test]
fn test_user_signature_committed_in_transactions() {
    // TODO: refactor this test to not reuse the same keys for user and authority signing
    let (a_sender, sender_sec): (_, AccountKeyPair) = get_key_pair();
    let (a_sender2, sender_sec2): (_, AccountKeyPair) = get_key_pair();

    let tx_data = TransactionData::new_transfer(
        a_sender2,
        random_object_ref(),
        a_sender,
        random_object_ref(),
        10000,
    );
    let transaction_a = Transaction::from_data(tx_data.clone(), &sender_sec);
    let transaction_b = Transaction::from_data(tx_data, &sender_sec2);
    let tx_digest_a = transaction_a.digest();
    let tx_digest_b = transaction_b.digest();
    assert_ne!(tx_digest_a, tx_digest_b);

    // Test hash non-equality
    // let mut hasher = DefaultHasher::new();
    // transaction_a.hash(&mut hasher);
    // let hash_a = hasher.finish();
    // let mut hasher = DefaultHasher::new();
    // transaction_b.hash(&mut hasher);
    // let hash_b = hasher.finish();
    // assert_ne!(hash_a, hash_b);

    // test equality
    assert_ne!(transaction_a, transaction_b)
}

#[test]
fn test_user_signature_committed_in_signed_transactions() {
    // TODO: refactor this test to not reuse the same keys for user and authority signing
    let (_a1, sec1): (_, AuthorityKeyPair) = get_key_pair();
    let (a_sender, sender_sec): (_, AccountKeyPair) = get_key_pair();
    let (a_sender2, sender_sec2): (_, AccountKeyPair) = get_key_pair();

    let tx_data = TransactionData::new_transfer(
        a_sender2,
        random_object_ref(),
        a_sender,
        random_object_ref(),
        10000,
    );
    let transaction_a = Transaction::from_data(tx_data.clone(), &sender_sec);
    let transaction_b = Transaction::from_data(tx_data, &sender_sec2);
    let signed_tx_a = SignedTransaction::new(
        0,
        transaction_a.data().clone(),
        &sec1,
        AuthorityPublicKeyBytes::from(sec1.public()),
    );
    let signed_tx_b = SignedTransaction::new(
        0,
        transaction_b.data().clone(),
        &sec1,
        AuthorityPublicKeyBytes::from(sec1.public()),
    );

    let tx_digest_a = signed_tx_a.digest();
    let tx_digest_b = signed_tx_b.digest();
    assert_ne!(tx_digest_a, tx_digest_b);

    // Ensure that signed tx verifies against the transaction with a correct user signature.
    let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();
    authorities.insert(AuthorityPublicKeyBytes::from(sec1.public()), 1);
    let committee = Committee::new(0, authorities.clone()).unwrap();
    assert!(signed_tx_a
        .auth_signature
        .verify(transaction_a.data(), &committee)
        .is_ok());
    assert!(signed_tx_a
        .auth_signature
        .verify(transaction_b.data(), &committee)
        .is_err());

    // // Test hash non-equality
    // let mut hasher = DefaultHasher::new();
    // signed_tx_a.hash(&mut hasher);
    // let hash_a = hasher.finish();
    // let mut hasher = DefaultHasher::new();
    // signed_tx_b.hash(&mut hasher);
    // let hash_b = hasher.finish();
    // assert_ne!(hash_a, hash_b);

    // test equality
    assert_ne!(signed_tx_a, signed_tx_b)
}

#[test]
fn test_user_signature_committed_in_checkpoints() {
    let (a1, _sec1): (_, AuthorityKeyPair) = get_key_pair();
    let (a_sender, sender_sec): (_, AccountKeyPair) = get_key_pair();
    let (a_sender2, sender_sec2): (_, AccountKeyPair) = get_key_pair();

    let tx_data = TransactionData::new_transfer(
        a_sender2,
        random_object_ref(),
        a_sender,
        random_object_ref(),
        10000,
    );

    let transaction_a = Transaction::from_data(tx_data.clone(), &sender_sec);
    let transaction_b = Transaction::from_data(tx_data, &sender_sec2);

    let tx_digest_a = transaction_a.digest();
    let tx_digest_b = transaction_b.digest();

    let effects_a = TransactionEffects {
        status: ExecutionStatus::Success,
        gas_used: GasCostSummary {
            computation_cost: 0,
            storage_cost: 0,
            storage_rebate: 0,
        },
        shared_objects: Vec::new(),
        transaction_digest: *tx_digest_a,
        created: Vec::new(),

        mutated: Vec::new(),
        unwrapped: Vec::new(),
        deleted: Vec::new(),
        wrapped: Vec::new(),
        gas_object: (random_object_ref(), Owner::AddressOwner(a1)),
        events: Vec::new(),
        dependencies: Vec::new(),
    };

    let mut effects_b = effects_a.clone();
    effects_b.transaction_digest = *tx_digest_b;

    let execution_digest_a = ExecutionDigests::new(*tx_digest_a, effects_a.digest());

    let execution_digest_b = ExecutionDigests::new(*tx_digest_b, effects_b.digest());

    let checkpoint_summary_a = CheckpointSummary::new(
        0,
        1,
        &CheckpointContents {
            transactions: vec![execution_digest_a],
        },
        None,
    );
    let checkpoint_summary_b = CheckpointSummary::new(
        0,
        1,
        &CheckpointContents {
            transactions: vec![execution_digest_b],
        },
        None,
    );

    assert_ne!(checkpoint_summary_a.digest(), checkpoint_summary_b.digest());

    // test non equality
    assert_ne!(checkpoint_summary_a, checkpoint_summary_b);
}

#[test]
fn verify_sender_signature_correctly_with_flag() {
    // set up authorities
    let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();
    let (_, sec1): (_, AuthorityKeyPair) = get_key_pair();
    let (_, sec2): (_, AuthorityKeyPair) = get_key_pair();
    authorities.insert(sec1.public().into(), 1);
    authorities.insert(sec2.public().into(), 0);
    let committee = Committee::new(0, authorities).unwrap();

    // create a receiver keypair with Secp256k1
    let receiver_kp = SuiKeyPair::Secp256k1SuiKeyPair(get_key_pair().1);
    let receiver_address = (&receiver_kp.public()).into();

    // create a sender keypair with Secp256k1
    let sender_kp = SuiKeyPair::Secp256k1SuiKeyPair(get_key_pair().1);

    // create a sender keypair with Ed25519
    let sender_kp_2 = SuiKeyPair::Ed25519SuiKeyPair(get_key_pair().1);

    // creates transaction envelope with user signed Secp256k1 signature
    let tx_data = TransactionData::new_transfer(
        receiver_address,
        random_object_ref(),
        (&sender_kp.public()).into(),
        random_object_ref(),
        10000,
    );

    let transaction = Transaction::from_data(tx_data, &sender_kp);

    // create tx also signed by authority
    let signed_tx = SignedTransaction::new(
        committee.epoch(),
        transaction.data().clone(),
        &sec1,
        AuthorityPublicKeyBytes::from(sec1.public()),
    );

    // signature contains the correct Secp256k1 flag
    assert_eq!(
        transaction.data().tx_signature.scheme().flag(),
        Secp256k1SuiSignature::SCHEME.flag()
    );

    // authority accepts signs tx after verification
    assert!(signed_tx
        .auth_signature
        .verify(transaction.data(), &committee)
        .is_ok());

    // creates transaction envelope with Ed25519 signature
    let transaction_1 = Transaction::from_data(
        TransactionData::new_transfer(
            receiver_address,
            random_object_ref(),
            (&sender_kp_2.public()).into(),
            random_object_ref(),
            10000,
        ),
        &sender_kp_2,
    );

    let signed_tx_1 = SignedTransaction::new(
        committee.epoch(),
        transaction_1.data().clone(),
        &sec1,
        AuthorityPublicKeyBytes::from(sec1.public()),
    );

    // signature contains the correct Ed25519 flag
    assert_eq!(
        transaction_1.data().tx_signature.scheme().flag(),
        Ed25519SuiSignature::SCHEME.flag()
    );

    // signature verified
    assert!(signed_tx_1
        .auth_signature
        .verify(transaction_1.data(), &committee)
        .is_ok());

    assert!(signed_tx_1
        .auth_signature
        .verify(transaction.data(), &committee)
        .is_err());
}
