// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::same_item_push)] // get_key_pair returns random elements

use std::time::Instant;

use crate::crypto::SignableBytes;
use crate::{
    base_types::*,
    crypto::{get_key_pair, AuthoritySignature},
    object::Object,
};

use super::*;

// Only relevant in a ser/de context : the `CertifiedTransaction` for a transaction is not unique
fn compare_certified_transactions(o1: &CertifiedTransaction, o2: &CertifiedTransaction) {
    assert_eq!(o1.digest(), o2.digest());
    // in this ser/de context it's relevant to compare signatures
    assert_eq!(o1.auth_sign_info.signatures, o2.auth_sign_info.signatures);
}

// Only relevant in a ser/de context : the `CertifiedTransaction` for a transaction is not unique
fn compare_object_info_responses(o1: &ObjectInfoResponse, o2: &ObjectInfoResponse) {
    assert_eq!(&o1.object().unwrap(), &o2.object().unwrap());
    assert_eq!(
        o1.object_and_lock.as_ref().unwrap().lock,
        o2.object_and_lock.as_ref().unwrap().lock
    );
    match (
        o1.parent_certificate.as_ref(),
        o2.parent_certificate.as_ref(),
    ) {
        (Some(cert1), Some(cert2)) => {
            assert_eq!(cert1.digest(), cert2.digest());
            assert_eq!(
                cert1.auth_sign_info.signatures,
                cert2.auth_sign_info.signatures
            );
        }
        (None, None) => (),
        _ => panic!("certificate structure between responses differs"),
    }
}

fn random_object_ref() -> ObjectRef {
    (
        ObjectID::random(),
        SequenceNumber::new(),
        ObjectDigest::new([0; 32]),
    )
}

#[test]
fn test_error() {
    let err = SuiError::UnknownSigner;
    let buf = serialize_error(&err);
    let result = deserialize_message(buf.as_slice());
    assert!(result.is_ok());
    if let SerializedMessage::Error(o) = result.unwrap() {
        assert!(*o == err);
    } else {
        panic!()
    }
}

#[test]
fn test_info_request() {
    let req1 = ObjectInfoRequest::latest_object_info_request(dbg_object_id(0x20), None);
    let req2 =
        ObjectInfoRequest::past_object_info_request(dbg_object_id(0x20), SequenceNumber::from(129));

    let buf1 = serialize_object_info_request(&req1);
    let buf2 = serialize_object_info_request(&req2);

    let result1 = deserialize_message(buf1.as_slice());
    let result2 = deserialize_message(buf2.as_slice());
    assert!(result1.is_ok());
    assert!(result2.is_ok());

    if let SerializedMessage::ObjectInfoReq(o) = result1.unwrap() {
        assert_eq!(*o, req1);
    } else {
        panic!()
    }
    if let SerializedMessage::ObjectInfoReq(o) = result2.unwrap() {
        assert_eq!(*o, req2);
    } else {
        panic!()
    }
}

#[test]
fn test_transaction() {
    let (sender_name, sender_key) = get_key_pair();

    let transfer_transaction = Transaction::from_data(
        TransactionData::new_transfer(
            dbg_addr(0x20),
            random_object_ref(),
            sender_name,
            random_object_ref(),
            10000,
        ),
        &sender_key,
    );

    let buf = serialize_transaction(&transfer_transaction);
    let result = deserialize_message(buf.as_slice());
    assert!(result.is_ok());
    if let SerializedMessage::Transaction(o) = result.unwrap() {
        assert!(*o == transfer_transaction);
    } else {
        panic!()
    }

    let (sender_name, sender_key) = get_key_pair();
    let transfer_transaction2 = Transaction::from_data(
        TransactionData::new_transfer(
            dbg_addr(0x20),
            random_object_ref(),
            sender_name,
            random_object_ref(),
            10000,
        ),
        &sender_key,
    );

    let buf = serialize_transaction(&transfer_transaction2);
    let result = deserialize_message(buf.as_slice());
    assert!(result.is_ok());
    if let SerializedMessage::Transaction(o) = result.unwrap() {
        assert!(*o == transfer_transaction2);
    } else {
        panic!()
    }
}

#[test]
fn test_vote() {
    let (sender_name, sender_key) = get_key_pair();
    let transaction = Transaction::from_data(
        TransactionData::new_transfer(
            dbg_addr(0x20),
            random_object_ref(),
            sender_name,
            random_object_ref(),
            50000,
        ),
        &sender_key,
    );

    let (_, authority_key) = get_key_pair();
    let vote = SignedTransaction::new(
        0,
        transaction,
        *authority_key.public_key_bytes(),
        &authority_key,
    );

    let buf = serialize_vote(&vote);
    let result = deserialize_message(buf.as_slice());
    assert!(result.is_ok());
    if let SerializedMessage::Vote(o) = result.unwrap() {
        assert!(*o == vote);
    } else {
        panic!()
    }
}

#[test]
fn test_cert() {
    let (sender_name, sender_key) = get_key_pair();
    let transaction = Transaction::from_data(
        TransactionData::new_transfer(
            dbg_addr(0x20),
            random_object_ref(),
            sender_name,
            random_object_ref(),
            10000,
        ),
        &sender_key,
    );
    let mut cert = CertifiedTransaction::new(transaction);

    for _ in 0..3 {
        let (_, authority_key) = get_key_pair();
        let sig = AuthoritySignature::new(&cert.data, &authority_key);

        cert.auth_sign_info
            .signatures
            .push((*authority_key.public_key_bytes(), sig));
    }

    let buf = serialize_cert(&cert);
    let result = deserialize_message(buf.as_slice());
    assert!(result.is_ok());
    if let SerializedMessage::Cert(o) = result.unwrap() {
        compare_certified_transactions(o.as_ref(), &cert);
    } else {
        panic!()
    }
}

#[test]
fn test_info_response() {
    let (sender_name, sender_key) = get_key_pair();
    let transaction = Transaction::from_data(
        TransactionData::new_transfer(
            dbg_addr(0x20),
            random_object_ref(),
            sender_name,
            random_object_ref(),
            10000,
        ),
        &sender_key,
    );

    let (_, auth_key) = get_key_pair();
    let vote = SignedTransaction::new(
        0,
        transaction.clone(),
        *auth_key.public_key_bytes(),
        &auth_key,
    );

    let mut cert = CertifiedTransaction::new(transaction);

    for _ in 0..3 {
        let (_, authority_key) = get_key_pair();
        let sig = AuthoritySignature::new(&cert.data, &authority_key);

        cert.auth_sign_info
            .signatures
            .push((*authority_key.public_key_bytes(), sig));
    }

    let object = Object::with_id_owner_for_testing(dbg_object_id(0x20), dbg_addr(0x20));
    let resp1 = ObjectInfoResponse {
        object_and_lock: Some(ObjectResponse {
            object: object.clone(),
            lock: Some(vote),
            layout: None,
        }),
        parent_certificate: None,
        requested_object_reference: Some(object.compute_object_reference()),
    };
    let resp2 = resp1.clone();
    let resp3 = resp1.clone();
    let resp4 = resp1.clone();

    for resp in [resp1, resp2, resp3, resp4].iter() {
        let buf = serialize_object_info_response(resp);
        let result = deserialize_message(buf.as_slice());
        assert!(result.is_ok());
        if let SerializedMessage::ObjectInfoResp(o) = result.unwrap() {
            compare_object_info_responses(o.as_ref(), resp);
        } else {
            panic!()
        }
    }
}

#[test]
fn test_time_transaction() {
    let (sender_name, sender_key) = get_key_pair();

    let mut buf = Vec::new();
    let now = Instant::now();
    for _ in 0..100 {
        let transfer_transaction = Transaction::from_data(
            TransactionData::new_transfer(
                dbg_addr(0x20),
                random_object_ref(),
                sender_name,
                random_object_ref(),
                50000,
            ),
            &sender_key,
        );
        serialize_transfer_transaction_into(&mut buf, &transfer_transaction).unwrap();
    }
    println!(
        "Write Transaction: {} microsec",
        now.elapsed().as_micros() / 100
    );

    let mut buf2 = buf.as_slice();
    let now = Instant::now();
    for _ in 0..100 {
        if let SerializedMessage::Transaction(transaction) = deserialize_message(&mut buf2).unwrap()
        {
            transaction.check_signature().unwrap();
        }
    }
    assert!(deserialize_message(&mut buf2).is_err());
    println!(
        "Read & Check Transaction: {} microsec",
        now.elapsed().as_micros() / 100
    );
}

#[test]
fn test_time_vote() {
    let (sender_name, sender_key) = get_key_pair();
    let transaction = Transaction::from_data(
        TransactionData::new_transfer(
            dbg_addr(0x20),
            random_object_ref(),
            sender_name,
            random_object_ref(),
            10000,
        ),
        &sender_key,
    );

    let (_, authority_key) = get_key_pair();

    let mut buf = Vec::new();
    let now = Instant::now();
    for _ in 0..100 {
        let vote = SignedTransaction::new(
            0,
            transaction.clone(),
            *authority_key.public_key_bytes(),
            &authority_key,
        );
        serialize_vote_into(&mut buf, &vote).unwrap();
    }
    println!("Write Vote: {} microsec", now.elapsed().as_micros() / 100);

    let mut buf2 = buf.as_slice();
    let now = Instant::now();
    for _ in 0..100 {
        if let SerializedMessage::Vote(vote) = deserialize_message(&mut buf2).unwrap() {
            vote.auth_sign_info
                .signature
                .check(&vote.data, vote.auth_sign_info.authority)
                .unwrap();
        }
    }
    assert!(deserialize_message(&mut buf2).is_err());
    println!(
        "Read & Quickcheck Vote: {} microsec",
        now.elapsed().as_micros() / 100
    );
}

#[test]
fn test_time_cert() {
    let count = 100;
    let (sender_name, sender_key) = get_key_pair();
    let transaction = Transaction::from_data(
        TransactionData::new_transfer(
            dbg_addr(0x20),
            random_object_ref(),
            sender_name,
            random_object_ref(),
            10000,
        ),
        &sender_key,
    );
    let mut cert = CertifiedTransaction::new(transaction);

    use std::collections::HashMap;
    let mut cache = HashMap::new();
    for _ in 0..7 {
        let (_, authority_key) = get_key_pair();
        let sig = AuthoritySignature::new(&cert.data, &authority_key);
        cert.auth_sign_info
            .signatures
            .push((*authority_key.public_key_bytes(), sig));
        cache.insert(
            *authority_key.public_key_bytes(),
            ed25519_dalek::PublicKey::from_bytes(authority_key.public_key_bytes().as_ref())
                .expect("No problem parsing key."),
        );
    }

    let mut buf = Vec::new();
    let now = Instant::now();

    for _ in 0..count {
        serialize_cert_into(&mut buf, &cert).unwrap();
    }
    println!("Write Cert: {} microsec", now.elapsed().as_micros() / count);

    let now = Instant::now();
    let mut buf2 = buf.as_slice();
    for _ in 0..count {
        if let SerializedMessage::Cert(cert) = deserialize_message(&mut buf2).unwrap() {
            AuthoritySignature::verify_batch(&cert.data, &cert.auth_sign_info.signatures, &cache)
                .unwrap();
        }
    }
    assert!(deserialize_message(buf2).is_err());
    println!(
        "Read & Quickcheck Cert: {} microsec",
        now.elapsed().as_micros() / count
    );
}

#[test]
fn test_signable_serde() -> Result<(), anyhow::Error> {
    let owner = SuiAddress::random_for_testing_only();
    let o1 = Object::with_id_owner_for_testing(ObjectID::random(), owner);
    let o2 = Object::with_id_owner_for_testing(ObjectID::random(), owner);
    let data = TransactionData::new_transfer(
        owner,
        o1.compute_object_reference(),
        owner,
        o2.compute_object_reference(),
        10000,
    );

    // Serialize
    let bytes = data.to_bytes();
    // Deserialize
    let deserialized_data = TransactionData::from_signable_bytes(&bytes)?;
    assert_eq!(data, deserialized_data);
    Ok(())
}
