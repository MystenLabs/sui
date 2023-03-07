// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;

use fastcrypto::traits::AggregateAuthenticator;
use fastcrypto::traits::KeyPair;
use move_core_types::language_storage::StructTag;
use roaring::RoaringBitmap;
use test_utils::messages::MAX_GAS;

use super::*;
use crate::base_types::random_object_ref;
use crate::crypto::bcs_signable_test::{get_obligation_input, Foo};
use crate::crypto::Secp256k1SuiSignature;
use crate::crypto::SuiKeyPair;
use crate::crypto::SuiSignature;
use crate::crypto::SuiSignatureInner;
use crate::crypto::VerificationObligation;
use crate::crypto::{
    get_key_pair, AccountKeyPair, AuthorityKeyPair, AuthorityPublicKeyBytes,
    AuthoritySignInfoTrait, SuiAuthoritySignature,
};
use crate::object::Owner;

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
        TransactionData::new_transfer_with_dummy_gas_price(
            _a2,
            random_object_ref(),
            a_sender,
            random_object_ref(),
            10000,
        ),
        Intent::default(),
        vec![&sender_sec],
    )
    .verify()
    .unwrap();

    let bad_transaction = VerifiedTransaction::new_unchecked(Transaction::from_data_and_signer(
        TransactionData::new_transfer_with_dummy_gas_price(
            _a2,
            random_object_ref(),
            a_sender,
            random_object_ref(),
            10000,
        ),
        Intent::default(),
        vec![&sender_sec2],
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
        TransactionData::new_transfer_with_dummy_gas_price(
            a2,
            random_object_ref(),
            a_sender,
            random_object_ref(),
            10000,
        ),
        Intent::default(),
        vec![&sender_sec],
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
        signatures.push(AuthoritySignInfo::new(
            0,
            &message,
            Intent::default().with_scope(IntentScope::SenderSignedTransaction),
            name,
            &sec,
        ));
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
                Intent::default().with_scope(IntentScope::SenderSignedTransaction),
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
        let sig = AuthoritySignature::new_secure(
            &IntentMessage::new(Intent::default(), message.clone()),
            &committee.epoch,
            &sec,
        );
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
    let mut obligation = VerificationObligation::default();
    let idx = obligation.add_message(
        &message,
        0, // Obligation added with correct epoch id.
        Intent::default().with_scope(IntentScope::SenderSignedTransaction),
    );
    let (_, sec): (_, AuthorityKeyPair) = get_key_pair();

    // Auth signtaure commits to epoch 0 verifies ok.
    let sig = AuthoritySignature::new_secure(
        &IntentMessage::new(
            Intent::default().with_scope(IntentScope::SenderSignedTransaction),
            message.clone(),
        ),
        &0,
        &sec,
    );
    let res = obligation.add_signature_and_public_key(&sig, sec.public(), idx);
    assert!(res.is_ok());
    assert!(obligation.verify_all().is_ok());

    // Auth signtaure commits to epoch 1 fails to verify.
    let mut obligation = VerificationObligation::default();
    let idx1 = obligation.add_message(
        &message,
        0, // Obligation added with correct epoch id.
        Intent::default().with_scope(IntentScope::SenderSignedTransaction),
    );
    let sig1 = AuthoritySignature::new_secure(
        &IntentMessage::new(
            Intent::default().with_scope(IntentScope::SenderSignedTransaction),
            message.clone(),
        ),
        &1,
        &sec,
    );
    let res = obligation.add_signature_and_public_key(&sig1, sec.public(), idx1);
    assert!(res.is_ok());
    assert!(obligation.verify_all().is_err());
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
            Intent::default().with_scope(IntentScope::SenderSignedTransaction),
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
            Intent::default().with_scope(IntentScope::SenderSignedTransaction),
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
            Intent::default().with_scope(IntentScope::SenderSignedTransaction),
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
            Intent::default().with_scope(IntentScope::SenderSignedTransaction),
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
        TransactionData::new_transfer_with_dummy_gas_price(
            sa1,
            random_object_ref(),
            sa2,
            random_object_ref(),
            10000,
        ),
        Intent::default(),
        vec![&ssec2],
    )
    .verify()
    .unwrap();

    let mut signed_tx = SignedTransaction::new(
        committee.epoch(),
        transaction.clone().into_message(),
        &sec1,
        AuthorityPublicKeyBytes::from(sec1.public()),
    );
    assert!(signed_tx.verify_signature(&committee).is_ok());

    let initial_digest = *signed_tx.digest();

    signed_tx
        .data_mut_for_testing()
        .intent_message
        .value
        .gas_data_mut()
        .budget += 1;

    // digest is cached
    assert_eq!(initial_digest, *signed_tx.digest());

    let serialized_tx = bcs::to_bytes(&signed_tx).unwrap();

    let deserialized_tx: SignedTransaction = bcs::from_bytes(&serialized_tx).unwrap();

    // cached digest was not serialized/deserialized
    assert_ne!(initial_digest, *deserialized_tx.digest());

    let effects = TransactionEffects::new_with_tx_and_gas(
        &transaction,
        (random_object_ref(), Owner::AddressOwner(a1)),
    );

    let mut signed_effects = SignedTransactionEffects::new(
        committee.epoch(),
        effects,
        &sec1,
        AuthorityPublicKeyBytes::from(sec1.public()),
    );

    let initial_effects_digest = *signed_effects.digest();
    signed_effects
        .data_mut_for_testing()
        .gas_cost_summary_mut_for_testing()
        .computation_cost += 1;

    // digest is cached
    assert_eq!(initial_effects_digest, *signed_effects.digest());

    let serialized_effects = bcs::to_bytes(&signed_effects).unwrap();

    let deserialized_effects: SignedTransactionEffects =
        bcs::from_bytes(&serialized_effects).unwrap();

    // cached digest was not serialized/deserialized
    assert_ne!(initial_effects_digest, *deserialized_effects.digest());
}

#[test]
fn test_user_signature_committed_in_transactions() {
    // TODO: refactor this test to not reuse the same keys for user and authority signing
    let (a_sender, sender_sec): (_, AccountKeyPair) = get_key_pair();
    let (a_sender2, sender_sec2): (_, AccountKeyPair) = get_key_pair();

    let tx_data = TransactionData::new_transfer_with_dummy_gas_price(
        a_sender2,
        random_object_ref(),
        a_sender,
        random_object_ref(),
        10000,
    );

    let mut tx_data_2 = tx_data.clone();
    tx_data_2.gas_data_mut().budget += 1;

    let transaction_a =
        Transaction::from_data_and_signer(tx_data.clone(), Intent::default(), vec![&sender_sec]);
    let transaction_b =
        Transaction::from_data_and_signer(tx_data, Intent::default(), vec![&sender_sec2]);
    let transaction_c =
        Transaction::from_data_and_signer(tx_data_2, Intent::default(), vec![&sender_sec2]);

    let tx_digest_a = transaction_a.digest();
    let tx_digest_b = transaction_b.digest();
    let tx_digest_c = transaction_c.digest();

    // The digest is the same for the same TransactionData even though the signature is different.
    assert_eq!(tx_digest_a, tx_digest_b);

    // The digest is the different for different TransactionData even though the signer is the same.
    assert_ne!(tx_digest_a, tx_digest_c);
    assert_ne!(tx_digest_b, tx_digest_c);

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

    let tx_data = TransactionData::new_transfer_with_dummy_gas_price(
        a_sender2,
        random_object_ref(),
        a_sender,
        random_object_ref(),
        10000,
    );
    let transaction_a =
        Transaction::from_data_and_signer(tx_data.clone(), Intent::default(), vec![&sender_sec])
            .verify()
            .unwrap();
    // transaction_b intentionally invalid (sender does not match signer).
    let transaction_b = VerifiedTransaction::new_unchecked(Transaction::from_data_and_signer(
        tx_data,
        Intent::default(),
        vec![&sender_sec2],
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
    // digest is derived from the same transaction data, not including the signature.
    assert_eq!(tx_digest_a, tx_digest_b);

    // Ensure that signed tx verifies against the transaction with a correct user signature.
    let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();
    authorities.insert(AuthorityPublicKeyBytes::from(sec1.public()), 1);
    let committee = Committee::new(0, authorities.clone()).unwrap();
    assert!(signed_tx_a
        .auth_sig()
        .verify_secure(
            transaction_a.data(),
            Intent::default().with_scope(IntentScope::SenderSignedTransaction),
            &committee
        )
        .is_ok());
    assert!(signed_tx_a
        .auth_sig()
        .verify_secure(
            transaction_b.data(),
            Intent::default().with_scope(IntentScope::SenderSignedTransaction),
            &committee
        )
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

fn signature_from_signer(
    data: TransactionData,
    intent: Intent,
    signer: &dyn Signer<Signature>,
) -> Signature {
    let intent_msg = IntentMessage::new(intent, data);
    Signature::new_secure(&intent_msg, signer)
}

#[test]
fn test_sponsored_transaction_message() {
    let sender_kp = SuiKeyPair::Ed25519(get_key_pair().1);
    let sender = (&sender_kp.public()).into();
    let sponsor_kp = SuiKeyPair::Ed25519(get_key_pair().1);
    let sponsor = (&sponsor_kp.public()).into();
    let kind = TransactionKind::Single(SingleTransactionKind::TransferObject(TransferObject {
        recipient: get_new_address::<AccountKeyPair>(),
        object_ref: random_object_ref(),
    }));
    let gas_obj_ref = random_object_ref();
    let gas_data = GasData {
        payment: vec![gas_obj_ref],
        owner: sponsor,
        price: DUMMY_GAS_PRICE,
        budget: 10000,
    };
    let tx_data = TransactionData::new_with_gas_data(kind, sender, gas_data.clone());
    let intent = Intent::default();
    let sender_sig: GenericSignature =
        signature_from_signer(tx_data.clone(), intent.clone(), &sender_kp).into();
    let sponsor_sig: GenericSignature =
        signature_from_signer(tx_data.clone(), intent.clone(), &sponsor_kp).into();
    let transaction = Transaction::from_generic_sig_data(
        tx_data.clone(),
        intent.clone(),
        vec![sender_sig.clone(), sponsor_sig.clone()],
    )
    .verify()
    .unwrap();

    assert_eq!(
        transaction.get_signer_sig_mapping().unwrap(),
        BTreeMap::from([(sender, &sender_sig), (sponsor, &sponsor_sig)]),
    );

    assert_eq!(transaction.sender_address(), sender,);
    assert_eq!(transaction.gas(), &[gas_obj_ref]);

    // Sig order does not matter
    let transaction = Transaction::from_generic_sig_data(
        tx_data.clone(),
        intent.clone(),
        vec![sponsor_sig.clone(), sender_sig.clone()],
    )
    .verify()
    .unwrap();

    // Test incomplete signature lists (missing sponsor sig)
    assert!(matches!(
        Transaction::from_generic_sig_data(
            tx_data.clone(),
            intent.clone(),
            vec![sender_sig.clone()],
        )
        .verify()
        .unwrap_err(),
        SuiError::SignerSignatureNumberMismatch { .. }
    ));

    // Test incomplete signature lists (missing sender sig)
    assert!(matches!(
        Transaction::from_generic_sig_data(
            tx_data.clone(),
            intent.clone(),
            vec![sponsor_sig.clone()],
        )
        .verify()
        .unwrap_err(),
        SuiError::SignerSignatureNumberMismatch { .. }
    ));

    // Test incomplete signature lists (more sigs than expected)
    let third_party_kp = SuiKeyPair::Ed25519(get_key_pair().1);
    let third_party_sig: GenericSignature =
        signature_from_signer(tx_data.clone(), intent.clone(), &third_party_kp).into();
    assert!(matches!(
        Transaction::from_generic_sig_data(
            tx_data.clone(),
            intent.clone(),
            vec![sender_sig, sponsor_sig.clone(), third_party_sig.clone()],
        )
        .verify()
        .unwrap_err(),
        SuiError::SignerSignatureNumberMismatch { .. }
    ));

    // Test irrelevant sigs
    assert!(matches!(
        Transaction::from_generic_sig_data(tx_data, intent, vec![sponsor_sig, third_party_sig],)
            .verify()
            .unwrap_err(),
        SuiError::SignerSignatureAbsent { .. }
    ));

    let tx = transaction.data().transaction_data();
    assert_eq!(tx.gas(), &[gas_obj_ref],);
    assert_eq!(tx.gas_data(), &gas_data,);
    assert_eq!(tx.sender(), sender,);
    assert_eq!(tx.gas_owner(), sponsor,);
}

#[test]
fn test_sponsored_transaction_validity_check() {
    let sender_kp = SuiKeyPair::Ed25519(get_key_pair().1);
    let sender = (&sender_kp.public()).into();
    let sponsor_kp = SuiKeyPair::Ed25519(get_key_pair().1);
    let sponsor = (&sponsor_kp.public()).into();

    // This is a sponsored transaction
    assert_ne!(sender, sponsor);
    let gas_data = GasData {
        payment: vec![random_object_ref()],
        owner: sponsor,
        price: DUMMY_GAS_PRICE,
        budget: 10000,
    };

    TransactionData::new_with_gas_data(
        TransactionKind::Single(SingleTransactionKind::TransferObject(TransferObject {
            recipient: get_new_address::<AccountKeyPair>(),
            object_ref: random_object_ref(),
        })),
        sender,
        gas_data.clone(),
    )
    .validity_check(&ProtocolConfig::get_for_max_version())
    .unwrap();

    TransactionData::new_with_gas_data(
        TransactionKind::Single(SingleTransactionKind::Call(MoveCall {
            package: ObjectID::random(),
            module: Identifier::new("random_module").unwrap(),
            function: Identifier::new("random_function").unwrap(),
            type_arguments: vec![],
            arguments: vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(
                random_object_ref(),
            ))],
        })),
        sender,
        gas_data.clone(),
    )
    .validity_check(&ProtocolConfig::get_for_max_version())
    .unwrap();

    TransactionData::new_with_gas_data(
        TransactionKind::Single(SingleTransactionKind::Publish(MoveModulePublish {
            modules: vec![vec![]],
        })),
        sender,
        gas_data.clone(),
    )
    .validity_check(&ProtocolConfig::get_for_max_version())
    .unwrap();

    TransactionData::new_with_gas_data(
        TransactionKind::Single(SingleTransactionKind::Pay(Pay {
            coins: vec![random_object_ref()],
            recipients: vec![SuiAddress::random_for_testing_only()],
            amounts: vec![100000],
        })),
        sender,
        gas_data.clone(),
    )
    .validity_check(&ProtocolConfig::get_for_max_version())
    .unwrap();

    // TransferSui cannot be sponsored
    assert_eq!(
        TransactionData::new_with_gas_data(
            TransactionKind::Single(SingleTransactionKind::TransferSui(TransferSui {
                recipient: SuiAddress::random_for_testing_only(),
                amount: Some(50000),
            })),
            sender,
            gas_data.clone(),
        )
        .validity_check(&ProtocolConfig::get_for_max_version())
        .unwrap_err(),
        UserInputError::UnsupportedSponsoredTransactionKind
    );

    // PaySui cannot be sponsored
    assert_eq!(
        TransactionData::new_with_gas_data(
            TransactionKind::Single(SingleTransactionKind::PaySui(PaySui {
                coins: gas_data.payment.clone(),
                recipients: vec![],
                amounts: vec![],
            })),
            sender,
            gas_data.clone(),
        )
        .validity_check(&ProtocolConfig::get_for_max_version())
        .unwrap_err(),
        UserInputError::UnsupportedSponsoredTransactionKind
    );

    // PayAllSui cannot be sponsored
    assert_eq!(
        TransactionData::new_with_gas_data(
            TransactionKind::Single(SingleTransactionKind::PayAllSui(PayAllSui {
                coins: gas_data.payment.clone(),
                recipient: SuiAddress::random_for_testing_only(),
            })),
            sender,
            gas_data.clone(),
        )
        .validity_check(&ProtocolConfig::get_for_max_version())
        .unwrap_err(),
        UserInputError::UnsupportedSponsoredTransactionKind
    );

    // Batch is non-sponsorable
    assert_eq!(
        TransactionData::new_with_gas_data(
            TransactionKind::Batch(vec![SingleTransactionKind::Pay(Pay {
                coins: vec![random_object_ref()],
                recipients: vec![SuiAddress::random_for_testing_only()],
                amounts: vec![100000],
            })]),
            sender,
            gas_data,
        )
        .validity_check(&ProtocolConfig::get_for_max_version())
        .unwrap_err(),
        UserInputError::UnsupportedSponsoredTransactionKind
    );
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
    let receiver_kp = SuiKeyPair::Secp256k1(get_key_pair().1);
    let receiver_address = (&receiver_kp.public()).into();

    // create a sender keypair with Secp256k1
    let sender_kp = SuiKeyPair::Secp256k1(get_key_pair().1);
    // and creates a corresponding transaction
    let tx_data = TransactionData::new_transfer_with_dummy_gas_price(
        receiver_address,
        random_object_ref(),
        (&sender_kp.public()).into(),
        random_object_ref(),
        10000,
    );

    // create a sender keypair with Ed25519
    let sender_kp_2 = SuiKeyPair::Ed25519(get_key_pair().1);
    let mut tx_data_2 = tx_data.clone();
    *tx_data_2.sender_mut() = (&sender_kp_2.public()).into();
    tx_data_2.gas_data_mut().owner = tx_data_2.sender();

    // create a sender keypair with Secp256r1
    let sender_kp_3 = SuiKeyPair::Secp256r1(get_key_pair().1);
    let mut tx_data_3 = tx_data.clone();
    *tx_data_3.sender_mut() = (&sender_kp_3.public()).into();
    tx_data_3.gas_data_mut().owner = tx_data_3.sender();

    let transaction =
        Transaction::from_data_and_signer(tx_data, Intent::default(), vec![&sender_kp])
            .verify()
            .unwrap();

    // create tx also signed by authority
    let signed_tx = SignedTransaction::new(
        committee.epoch(),
        transaction.clone().into_message(),
        &sec1,
        AuthorityPublicKeyBytes::from(sec1.public()),
    );

    let s = match &transaction.data().tx_signatures[0] {
        GenericSignature::Signature(s) => s,
        _ => panic!("invalid"),
    };
    // signature contains the correct Secp256k1 flag
    assert_eq!(s.scheme().flag(), Secp256k1SuiSignature::SCHEME.flag());

    // authority accepts signs tx after verification
    assert!(signed_tx
        .auth_sig()
        .verify_secure(
            transaction.data(),
            Intent::default().with_scope(IntentScope::SenderSignedTransaction),
            &committee
        )
        .is_ok());

    let transaction_1 =
        Transaction::from_data_and_signer(tx_data_2, Intent::default(), vec![&sender_kp_2])
            .verify()
            .unwrap();

    let signed_tx_1 = SignedTransaction::new(
        committee.epoch(),
        transaction_1.clone().into_message(),
        &sec1,
        AuthorityPublicKeyBytes::from(sec1.public()),
    );
    let s = match &transaction_1.data().tx_signatures[0] {
        GenericSignature::Signature(s) => s,
        _ => panic!("unexpected signature scheme"),
    };

    // signature contains the correct Ed25519 flag
    assert_eq!(s.scheme().flag(), Ed25519SuiSignature::SCHEME.flag());

    // signature verified
    assert!(signed_tx_1
        .auth_sig()
        .verify_secure(
            transaction_1.data(),
            Intent::default().with_scope(IntentScope::SenderSignedTransaction),
            &committee
        )
        .is_ok());

    assert!(signed_tx_1
        .auth_sig()
        .verify_secure(
            transaction.data(),
            Intent::default().with_scope(IntentScope::SenderSignedTransaction),
            &committee
        )
        .is_err());

    // create transaction with r1 signer
    let tx_3 = Transaction::from_data_and_signer(tx_data_3, Intent::default(), vec![&sender_kp_3]);
    let tx_31 = tx_3.clone();
    let tx_32 = tx_3.clone();

    // r1 signature tx verifies ok
    assert!(tx_3.verify().is_ok());
    let verified_tx_3 = tx_31.verify().unwrap();
    // r1 signature verified and accepted by authority
    let signed_tx_3 = SignedTransaction::new(
        committee.epoch(),
        verified_tx_3.into_message(),
        &sec1,
        AuthorityPublicKeyBytes::from(sec1.public()),
    );
    assert!(signed_tx_3
        .auth_sig()
        .verify_secure(
            tx_32.data(),
            Intent::default().with_scope(IntentScope::SenderSignedTransaction),
            &committee
        )
        .is_ok());
}

#[test]
fn test_change_epoch_transaction() {
    let tx = VerifiedTransaction::new_change_epoch(1, ProtocolVersion::MIN, 0, 0, 0, 0, vec![]);
    assert!(tx.contains_shared_object());
    assert_eq!(
        tx.shared_input_objects().next().unwrap(),
        SharedInputObject {
            id: SUI_SYSTEM_STATE_OBJECT_ID,
            initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
            mutable: true,
        }
    );
    assert!(tx.is_system_tx());
    assert_eq!(
        tx.data()
            .intent_message
            .value
            .input_objects()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn test_consensus_commit_prologue_transaction() {
    let tx = VerifiedTransaction::new_consensus_commit_prologue(0, 0, 42);
    assert!(tx.contains_shared_object());
    assert_eq!(
        tx.shared_input_objects().next().unwrap(),
        SharedInputObject {
            id: SUI_CLOCK_OBJECT_ID,
            initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
            mutable: true,
        },
    );
    assert!(tx.is_system_tx());
    assert_eq!(
        tx.data()
            .intent_message
            .value
            .input_objects()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn test_move_input_objects() {
    let package = ObjectID::random();
    let p1 = ObjectID::random();
    let p2 = ObjectID::random();
    let p3 = ObjectID::random();
    let p4 = ObjectID::random();
    let p5 = ObjectID::random();
    let o1 = random_object_ref();
    let o2 = random_object_ref();
    let o3 = random_object_ref();
    let shared = random_object_ref();

    let gas_object_ref = random_object_ref();
    let mk_st = |package: ObjectID, type_args| {
        TypeTag::Struct(Box::new(StructTag {
            address: package.into(),
            module: Identifier::new("foo").unwrap(),
            name: Identifier::new("bar").unwrap(),
            type_params: type_args,
        }))
    };
    let t1 = mk_st(p1, vec![]);
    let t2 = mk_st(p2, vec![mk_st(p3, vec![]), mk_st(p4, vec![])]);
    let t3 = TypeTag::Vector(Box::new(mk_st(p5, vec![])));
    let type_args = vec![t1, t2, t3];
    let args = vec![
        CallArg::Object(ObjectArg::ImmOrOwnedObject(o1)),
        CallArg::ObjVec(vec![
            ObjectArg::ImmOrOwnedObject(o2),
            ObjectArg::ImmOrOwnedObject(o3),
        ]),
        CallArg::Object(ObjectArg::SharedObject {
            id: shared.0,
            initial_shared_version: shared.1,
            mutable: true,
        }),
    ];

    let data = dummy_move_call(package, "foo", "bar", type_args, gas_object_ref, args);
    let mut input_objects = data.input_objects().unwrap();
    macro_rules! rem {
        ($exp:expr) => {{
            let idx = input_objects
                .iter()
                .position(|x| x == &$exp)
                .expect(std::concat!(
                    "Unbound input object: ",
                    std::stringify!($exp)
                ));
            input_objects.swap_remove(idx);
        }};
    }
    rem!(InputObjectKind::MovePackage(package));
    rem!(InputObjectKind::MovePackage(p1));
    rem!(InputObjectKind::MovePackage(p2));
    rem!(InputObjectKind::MovePackage(p3));
    rem!(InputObjectKind::MovePackage(p4));
    rem!(InputObjectKind::MovePackage(p5));
    rem!(InputObjectKind::ImmOrOwnedMoveObject(o1));
    rem!(InputObjectKind::ImmOrOwnedMoveObject(o2));
    rem!(InputObjectKind::ImmOrOwnedMoveObject(o3));
    rem!(InputObjectKind::SharedMoveObject {
        id: shared.0,
        initial_shared_version: shared.1,
        mutable: true,
    });
    rem!(InputObjectKind::ImmOrOwnedMoveObject(gas_object_ref));
    assert!(input_objects.is_empty());
}

fn dummy_move_call(
    package: ObjectID,
    module: &str,
    function: &str,
    type_args: Vec<TypeTag>,
    gas_object_ref: ObjectRef,
    args: Vec<CallArg>,
) -> TransactionData {
    TransactionData::new_move_call_with_dummy_gas_price(
        SuiAddress::random_for_testing_only(),
        package,
        Identifier::new(module).unwrap(),
        Identifier::new(function).unwrap(),
        type_args,
        gas_object_ref,
        args,
        MAX_GAS,
    )
}

#[test]
fn test_unique_input_objects() {
    let package = ObjectID::random();
    let p1 = ObjectID::random();
    let p2 = ObjectID::random();
    let p3 = ObjectID::random();
    let p4 = ObjectID::random();
    let p5 = ObjectID::random();
    let o1 = random_object_ref();
    let o2 = random_object_ref();
    let o3 = random_object_ref();
    let shared = random_object_ref();

    let mk_st = |package: ObjectID, type_args| {
        TypeTag::Struct(Box::new(StructTag {
            address: package.into(),
            module: Identifier::new("foo").unwrap(),
            name: Identifier::new("bar").unwrap(),
            type_params: type_args,
        }))
    };
    let t1 = mk_st(p1, vec![]);
    let t2 = mk_st(p2, vec![mk_st(p3, vec![]), mk_st(p4, vec![])]);
    let t3 = TypeTag::Vector(Box::new(mk_st(p5, vec![])));
    let type_args = vec![t1, t2, t3];
    let args_1 = vec![
        CallArg::Object(ObjectArg::ImmOrOwnedObject(o1)),
        CallArg::ObjVec(vec![
            ObjectArg::ImmOrOwnedObject(o2),
            ObjectArg::ImmOrOwnedObject(o3),
        ]),
    ];
    let args_2 = vec![CallArg::Object(ObjectArg::SharedObject {
        id: shared.0,
        initial_shared_version: shared.1,
        mutable: true,
    })];

    let sender_kp = SuiKeyPair::Ed25519(get_key_pair().1);
    let sender = (&sender_kp.public()).into();

    let gas_object_ref = random_object_ref();
    let gas_data = GasData {
        payment: vec![gas_object_ref],
        owner: sender,
        price: DUMMY_GAS_PRICE,
        budget: 10000,
    };

    let transaction_data = TransactionData::new_with_gas_data(
        TransactionKind::Batch(vec![
            SingleTransactionKind::Call(MoveCall {
                package,
                module: Identifier::new("test_module").unwrap(),
                function: Identifier::new("test_function").unwrap(),
                type_arguments: type_args.clone(),
                arguments: args_1,
            }),
            SingleTransactionKind::Call(MoveCall {
                package,
                module: Identifier::new("test_module").unwrap(),
                function: Identifier::new("test_function").unwrap(),
                type_arguments: type_args,
                arguments: args_2,
            }),
        ]),
        sender,
        gas_data,
    );

    let input_objects = transaction_data.input_objects().unwrap();
    let input_objects_map: BTreeSet<_> = input_objects.iter().cloned().collect();
    assert_eq!(
        input_objects.len(),
        input_objects_map.len(),
        "Duplicates in {:?}",
        input_objects
    );
}
