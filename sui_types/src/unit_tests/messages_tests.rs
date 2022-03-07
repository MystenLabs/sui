// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::crypto::get_key_pair;

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
    let mut authorities = BTreeMap::new();
    // TODO: refactor this test to not reuse the same keys for user and authority signing
    let (a1, sec1) = get_key_pair();
    let (a2, sec2) = get_key_pair();
    let (_, sec3) = get_key_pair();

    authorities.insert(
        /* address */ *sec1.public_key_bytes(),
        /* voting right */ 1,
    );
    authorities.insert(
        /* address */ *sec2.public_key_bytes(),
        /* voting right */ 0,
    );
    let committee = Committee::new(authorities);

    let transaction = Transaction::from_data(
        TransactionData::new_transfer(a2, random_object_ref(), a1, random_object_ref()),
        &sec1,
    );
    let bad_transaction = Transaction::from_data(
        TransactionData::new_transfer(a2, random_object_ref(), a1, random_object_ref()),
        &sec2,
    );

    let v = SignedTransaction::new(transaction.clone(), *sec1.public_key_bytes(), &sec1);
    assert!(v.check(&committee).is_ok());

    let v = SignedTransaction::new(transaction.clone(), *sec2.public_key_bytes(), &sec2);
    assert!(v.check(&committee).is_err());

    let v = SignedTransaction::new(transaction, *sec3.public_key_bytes(), &sec3);
    assert!(v.check(&committee).is_err());

    let v = SignedTransaction::new(bad_transaction, *sec1.public_key_bytes(), &sec1);
    assert!(v.check(&committee).is_err());
}

#[test]
fn test_certificates() {
    let (a1, sec1) = get_key_pair();
    let (a2, sec2) = get_key_pair();
    let (_, sec3) = get_key_pair();

    let mut authorities = BTreeMap::new();
    authorities.insert(
        /* address */ *sec1.public_key_bytes(),
        /* voting right */ 1,
    );
    authorities.insert(
        /* address */ *sec2.public_key_bytes(),
        /* voting right */ 1,
    );
    let committee = Committee::new(authorities);

    let transaction = Transaction::from_data(
        TransactionData::new_transfer(a2, random_object_ref(), a1, random_object_ref()),
        &sec1,
    );
    let bad_transaction = Transaction::from_data(
        TransactionData::new_transfer(a2, random_object_ref(), a1, random_object_ref()),
        &sec2,
    );

    let v1 = SignedTransaction::new(transaction.clone(), *sec1.public_key_bytes(), &sec1);
    let v2 = SignedTransaction::new(transaction.clone(), *sec2.public_key_bytes(), &sec2);
    let v3 = SignedTransaction::new(transaction.clone(), *sec3.public_key_bytes(), &sec3);

    let mut builder = SignatureAggregator::try_new(transaction.clone(), &committee).unwrap();
    assert!(builder
        .append(v1.authority, v1.signature)
        .unwrap()
        .is_none());
    let mut c = builder.append(v2.authority, v2.signature).unwrap().unwrap();
    assert!(c.check(&committee).is_ok());
    c.signatures.pop();
    assert!(c.check(&committee).is_err());

    let mut builder = SignatureAggregator::try_new(transaction, &committee).unwrap();
    assert!(builder
        .append(v1.authority, v1.signature)
        .unwrap()
        .is_none());
    assert!(builder.append(v3.authority, v3.signature).is_err());

    assert!(SignatureAggregator::try_new(bad_transaction, &committee).is_err());
}
