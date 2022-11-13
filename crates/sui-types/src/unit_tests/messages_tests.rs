// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;

use fastcrypto::traits::AggregateAuthenticator;
use fastcrypto::traits::KeyPair;
use roaring::RoaringBitmap;

use crate::base_types::random_object_ref;
use crate::crypto::bcs_signable_test::{get_obligation_input, Foo};
use crate::crypto::Secp256k1SuiSignature;
use crate::crypto::SuiKeyPair;
use crate::crypto::{
    get_key_pair, AccountKeyPair, AuthorityKeyPair, AuthorityPublicKeyBytes,
    AuthoritySignInfoTrait, SuiAuthoritySignature,
};
use crate::messages_checkpoint::CheckpointContents;
use crate::messages_checkpoint::CheckpointSummary;
use crate::object::Owner;

use super::*;

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

    let transaction = Transaction::from_data_and_signer(
        TransactionData::new_transfer(
            _a2,
            random_object_ref(),
            a_sender,
            random_object_ref(),
            10000,
        ),
        &sender_sec,
    )
    .verify()
    .unwrap();

    let bad_transaction = VerifiedTransaction::new_unchecked(Transaction::from_data_and_signer(
        TransactionData::new_transfer(
            _a2,
            random_object_ref(),
            a_sender,
            random_object_ref(),
            10000,
        ),
        &sender_sec2,
    ));

    let v = SignedTransaction::new(
        committee.epoch(),
        transaction.clone().into_message(),
        &sec1,
        AuthorityPublicKeyBytes::from(sec1.public()),
    );
    assert!(v.verify(&committee).is_ok());

    let v = SignedTransaction::new(
        committee.epoch(),
        transaction.clone().into_message(),
        &sec2,
        AuthorityPublicKeyBytes::from(sec2.public()),
    );
    assert!(v.verify(&committee).is_err());

    let v = SignedTransaction::new(
        committee.epoch(),
        transaction.into_message(),
        &sec3,
        AuthorityPublicKeyBytes::from(sec3.public()),
    );
    assert!(v.verify(&committee).is_err());

    let v = SignedTransaction::new(
        committee.epoch(),
        bad_transaction.into_message(),
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

    let transaction = Transaction::from_data_and_signer(
        TransactionData::new_transfer(
            a2,
            random_object_ref(),
            a_sender,
            random_object_ref(),
            10000,
        ),
        &sender_sec,
    )
    .verify()
    .unwrap();

    let v1 = SignedTransaction::new(
        committee.epoch(),
        transaction.clone().into_message(),
        &sec1,
        AuthorityPublicKeyBytes::from(sec1.public()),
    );
    let v2 = SignedTransaction::new(
        committee.epoch(),
        transaction.clone().into_message(),
        &sec2,
        AuthorityPublicKeyBytes::from(sec2.public()),
    );
    let v3 = SignedTransaction::new(
        committee.epoch(),
        transaction.clone().into_message(),
        &sec3,
        AuthorityPublicKeyBytes::from(sec3.public()),
    );

    let mut sigs = vec![v1.auth_sig().clone()];
    assert!(CertifiedTransaction::new(
        transaction.clone().into_message(),
        sigs.clone(),
        &committee
    )
    .is_err());
    sigs.push(v2.auth_sig().clone());
    let c =
        CertifiedTransaction::new(transaction.clone().into_message(), sigs, &committee).unwrap();
    assert!(c.verify_signature(&committee).is_ok());

    let sigs = vec![v1.auth_sig().clone(), v3.auth_sig().clone()];

    assert!(CertifiedTransaction::new(transaction.into_message(), sigs, &committee).is_err());
}

#[test]
fn test_new_with_signatures() {
    let message: Foo = Foo("some data".to_string());
    let mut signatures: Vec<AuthoritySignInfo> = Vec::new();
    let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();

    for _ in 0..5 {
        let (_, sec): (_, AuthorityKeyPair) = get_key_pair();
        let name = AuthorityPublicKeyBytes::from(sec.public());
        signatures.push(AuthoritySignInfo::new(0, &message, name, &sec));
        authorities.insert(name, 1);
    }
    let (_, sec): (_, AuthorityKeyPair) = get_key_pair();
    authorities.insert(AuthorityPublicKeyBytes::from(sec.public()), 1);

    let committee = Committee::new(0, authorities.clone()).unwrap();
    let quorum =
        AuthorityStrongQuorumSignInfo::new_from_auth_sign_infos(signatures.clone(), &committee)
            .unwrap();

    let sig_clone = signatures.clone();
    let mut alphabetical_authorities = sig_clone
        .iter()
        .map(|a| &a.authority)
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
    let message: Foo = Foo("some data".to_string());
    let mut signatures: Vec<AuthoritySignInfo> = Vec::new();
    let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();

    for i in 0..5 {
        let (_, sec): (_, AuthorityKeyPair) = get_key_pair();
        let name = AuthorityPublicKeyBytes::from(sec.public());
        authorities.insert(name, 1);
        if i < 4 {
            signatures.push(AuthoritySignInfo::new(
                0,
                &Foo("some data".to_string()),
                name,
                &sec,
            ))
        };
    }

    let committee = Committee::new(0, authorities.clone()).unwrap();
    let mut quorum =
        AuthorityStrongQuorumSignInfo::new_from_auth_sign_infos(signatures, &committee).unwrap();
    {
        let (_, sec): (_, AuthorityKeyPair) = get_key_pair();
        let sig = AuthoritySignature::new(&message, committee.epoch, &sec);
        quorum.signature.add_signature(sig).unwrap();
    }
    let (mut obligation, idx) = get_obligation_input(&message);
    assert!(quorum
        .add_to_verification_obligation(&committee, &mut obligation, idx)
        .is_ok());
    assert!(obligation.verify_all().is_err());
}

#[test]
fn test_auth_sig_commit_to_wrong_epoch_id_fail() {
    let message: Foo = Foo("some data".to_string());
    let mut signatures: Vec<AuthoritySignInfo> = Vec::new();
    let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();

    for _ in 0..5 {
        let (_, sec): (_, AuthorityKeyPair) = get_key_pair();
        let name = AuthorityPublicKeyBytes::from(sec.public());
        authorities.insert(name, 1);
        signatures.push(AuthoritySignInfo::new(
            1,
            &Foo("some data".to_string()),
            name,
            &sec,
        ));
    }
    // committee set up with epoch 1
    let committee = Committee::new(1, authorities.clone()).unwrap();
    let mut quorum =
        AuthorityStrongQuorumSignInfo::new_from_auth_sign_infos(signatures, &committee).unwrap();
    {
        let (_, sec): (_, AuthorityKeyPair) = get_key_pair();
        // signature commits to epoch 0
        let sig = AuthoritySignature::new(&message, 1, &sec);
        quorum.signature.add_signature(sig).unwrap();
    }
    let (mut obligation, idx) = get_obligation_input(&message);
    assert!(quorum
        .add_to_verification_obligation(&committee, &mut obligation, idx)
        .is_ok());
    assert_eq!(
        obligation.verify_all(),
        Err(SuiError::InvalidSignature {
            error: "General cryptographic error".to_string()
        })
    );
}

#[test]
fn test_bitmap_out_of_range() {
    let message: Foo = Foo("some data".to_string());
    let mut signatures: Vec<AuthoritySignInfo> = Vec::new();
    let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();
    for _ in 0..5 {
        let (_, sec): (_, AuthorityKeyPair) = get_key_pair();
        let name = AuthorityPublicKeyBytes::from(sec.public());
        authorities.insert(name, 1);
        signatures.push(AuthoritySignInfo::new(
            0,
            &Foo("some data".to_string()),
            name,
            &sec,
        ));
    }

    let committee = Committee::new(0, authorities.clone()).unwrap();
    let mut quorum =
        AuthorityStrongQuorumSignInfo::new_from_auth_sign_infos(signatures, &committee).unwrap();

    // Insert outside of range
    quorum.signers_map.insert(10);

    let (mut obligation, idx) = get_obligation_input(&message);
    assert!(quorum
        .add_to_verification_obligation(&committee, &mut obligation, idx)
        .is_err());
}

#[test]
fn test_reject_extra_public_key() {
    let message: Foo = Foo("some data".to_string());
    let mut signatures: Vec<AuthoritySignInfo> = Vec::new();
    let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();
    // TODO: quite duplicated code in this file (4 times).
    for _ in 0..5 {
        let (_, sec): (_, AuthorityKeyPair) = get_key_pair();
        let name = AuthorityPublicKeyBytes::from(sec.public());
        authorities.insert(name, 1);
        signatures.push(AuthoritySignInfo::new(
            0,
            &Foo("some data".to_string()),
            name,
            &sec,
        ));
    }

    signatures.sort_by_key(|k| k.authority);

    let used_signatures: Vec<AuthoritySignInfo> = vec![
        signatures[0].clone(),
        signatures[1].clone(),
        signatures[2].clone(),
        signatures[3].clone(),
    ];

    let committee = Committee::new(0, authorities.clone()).unwrap();
    let mut quorum =
        AuthorityStrongQuorumSignInfo::new_from_auth_sign_infos(used_signatures, &committee)
            .unwrap();

    quorum.signers_map.insert(3);

    let (mut obligation, idx) = get_obligation_input(&message);
    assert!(quorum
        .add_to_verification_obligation(&committee, &mut obligation, idx)
        .is_ok());
}

#[test]
fn test_reject_reuse_signatures() {
    let message: Foo = Foo("some data".to_string());
    let mut signatures: Vec<AuthoritySignInfo> = Vec::new();
    let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();
    for _ in 0..5 {
        let (_, sec): (_, AuthorityKeyPair) = get_key_pair();
        let name = AuthorityPublicKeyBytes::from(sec.public());
        authorities.insert(name, 1);
        signatures.push(AuthoritySignInfo::new(
            0,
            &Foo("some data".to_string()),
            name,
            &sec,
        ));
    }

    let used_signatures: Vec<AuthoritySignInfo> = vec![
        signatures[0].clone(),
        signatures[1].clone(),
        signatures[2].clone(),
        signatures[2].clone(),
    ];

    let committee = Committee::new(0, authorities.clone()).unwrap();
    let quorum =
        AuthorityStrongQuorumSignInfo::new_from_auth_sign_infos(used_signatures, &committee)
            .unwrap();

    let (mut obligation, idx) = get_obligation_input(&message);
    assert!(quorum
        .add_to_verification_obligation(&committee, &mut obligation, idx)
        .is_err());
}

#[test]
fn test_empty_bitmap() {
    let message: Foo = Foo("some data".to_string());
    let mut signatures: Vec<AuthoritySignInfo> = Vec::new();
    let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();
    for _ in 0..5 {
        let (_, sec): (_, AuthorityKeyPair) = get_key_pair();
        let name = AuthorityPublicKeyBytes::from(sec.public());
        authorities.insert(name, 1);
        signatures.push(AuthoritySignInfo::new(
            0,
            &Foo("some data".to_string()),
            name,
            &sec,
        ));
    }

    let committee = Committee::new(0, authorities.clone()).unwrap();
    let mut quorum =
        AuthorityStrongQuorumSignInfo::new_from_auth_sign_infos(signatures, &committee).unwrap();
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

    let transaction = Transaction::from_data_and_signer(
        TransactionData::new_transfer(sa1, random_object_ref(), sa2, random_object_ref(), 10000),
        &ssec2,
    )
    .verify()
    .unwrap();

    let mut signed_tx = SignedTransaction::new(
        committee.epoch(),
        transaction.into_message(),
        &sec1,
        AuthorityPublicKeyBytes::from(sec1.public()),
    );
    assert!(signed_tx.verify_signature(&committee).is_ok());

    let initial_digest = *signed_tx.digest();

    signed_tx.data_mut_for_testing().data.gas_budget += 1;

    // digest is cached
    assert_eq!(initial_digest, *signed_tx.digest());

    let serialized_tx = bincode::serialize(&signed_tx).unwrap();

    let deserialized_tx: SignedTransaction = bincode::deserialize(&serialized_tx).unwrap();

    // cached digest was not serialized/deserialized
    assert_ne!(initial_digest, *deserialized_tx.digest());

    let effects = TransactionEffects {
        transaction_digest: initial_digest,
        gas_object: (random_object_ref(), Owner::AddressOwner(a1)),
        ..Default::default()
    };

    let mut signed_effects = SignedTransactionEffects::new(
        committee.epoch(),
        effects,
        &sec1,
        AuthorityPublicKeyBytes::from(sec1.public()),
    );

    let initial_effects_digest = *signed_effects.digest();
    signed_effects
        .data_mut_for_testing()
        .gas_used
        .computation_cost += 1;

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
    let transaction_a = Transaction::from_data_and_signer(tx_data.clone(), &sender_sec);
    let transaction_b = Transaction::from_data_and_signer(tx_data, &sender_sec2);
    let tx_digest_a = transaction_a.digest();
    let tx_digest_b = transaction_b.digest();
    assert_ne!(tx_digest_a, tx_digest_b);

    // Test hash non-equality
    let mut hasher = DefaultHasher::new();
    transaction_a.hash(&mut hasher);
    let hash_a = hasher.finish();
    let mut hasher = DefaultHasher::new();
    transaction_b.hash(&mut hasher);
    let hash_b = hasher.finish();
    assert_ne!(hash_a, hash_b);

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
    let transaction_a = Transaction::from_data_and_signer(tx_data.clone(), &sender_sec)
        .verify()
        .unwrap();
    // transaction_b intentionally invalid (sender does not match signer).
    let transaction_b = VerifiedTransaction::new_unchecked(Transaction::from_data_and_signer(
        tx_data,
        &sender_sec2,
    ));

    let signed_tx_a = SignedTransaction::new(
        0,
        transaction_a.clone().into_message(),
        &sec1,
        AuthorityPublicKeyBytes::from(sec1.public()),
    );
    let signed_tx_b = SignedTransaction::new(
        0,
        transaction_b.clone().into_message(),
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
        .auth_sig()
        .verify(transaction_a.data(), &committee)
        .is_ok());
    assert!(signed_tx_a
        .auth_sig()
        .verify(transaction_b.data(), &committee)
        .is_err());

    // Test hash non-equality
    let mut hasher = DefaultHasher::new();
    signed_tx_a.hash(&mut hasher);
    let hash_a = hasher.finish();
    let mut hasher = DefaultHasher::new();
    signed_tx_b.hash(&mut hasher);
    let hash_b = hasher.finish();
    assert_ne!(hash_a, hash_b);

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

    let transaction_a = Transaction::from_data_and_signer(tx_data.clone(), &sender_sec);
    let transaction_b = Transaction::from_data_and_signer(tx_data, &sender_sec2);

    let tx_digest_a = transaction_a.digest();
    let tx_digest_b = transaction_b.digest();

    let effects_a = TransactionEffects {
        transaction_digest: *tx_digest_a,
        gas_object: (random_object_ref(), Owner::AddressOwner(a1)),
        ..Default::default()
    };

    let mut effects_b = effects_a.clone();
    effects_b.transaction_digest = *tx_digest_b;

    let execution_digest_a = ExecutionDigests::new(*tx_digest_a, effects_a.digest());

    let execution_digest_b = ExecutionDigests::new(*tx_digest_b, effects_b.digest());

    let checkpoint_summary_a = CheckpointSummary::new(
        0,
        1,
        &CheckpointContents::new_with_causally_ordered_transactions(
            [execution_digest_a].into_iter(),
        ),
        None,
        effects_a.gas_used,
        None,
    );
    let checkpoint_summary_b = CheckpointSummary::new(
        0,
        1,
        &CheckpointContents::new_with_causally_ordered_transactions(
            [execution_digest_b].into_iter(),
        ),
        None,
        effects_b.gas_used,
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

    let transaction = Transaction::from_data_and_signer(tx_data, &sender_kp)
        .verify()
        .unwrap();

    // create tx also signed by authority
    let signed_tx = SignedTransaction::new(
        committee.epoch(),
        transaction.clone().into_message(),
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
        .auth_sig()
        .verify(transaction.data(), &committee)
        .is_ok());

    // creates transaction envelope with Ed25519 signature
    let transaction_1 = Transaction::from_data_and_signer(
        TransactionData::new_transfer(
            receiver_address,
            random_object_ref(),
            (&sender_kp_2.public()).into(),
            random_object_ref(),
            10000,
        ),
        &sender_kp_2,
    )
    .verify()
    .unwrap();

    let signed_tx_1 = SignedTransaction::new(
        committee.epoch(),
        transaction_1.clone().into_message(),
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
        .auth_sig()
        .verify(transaction_1.data(), &committee)
        .is_ok());

    assert!(signed_tx_1
        .auth_sig()
        .verify(transaction.data(), &committee)
        .is_err());
}
