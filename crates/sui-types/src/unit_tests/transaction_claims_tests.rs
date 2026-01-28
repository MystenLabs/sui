// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::base_types::{ObjectID, SequenceNumber};

#[test]
fn test_transaction_claim_address_aliases() {
    let aliases = nonempty![(0u8, Some(SequenceNumber::from(1))), (1u8, None)];

    let claim = TransactionClaim::AddressAliasesV2(aliases.clone());

    match &claim {
        TransactionClaim::AddressAliasesV2(a) => {
            assert_eq!(a.len(), 2);
            assert_eq!(a.head.0, 0u8); // signature index 0
            assert_eq!(a.head.1, Some(SequenceNumber::from(1)));
        }
        _ => panic!("Expected AddressAliasesV2 claim"),
    }
}

#[test]
fn test_transaction_claim_immutable_objects() {
    let obj1 = ObjectID::random();
    let obj2 = ObjectID::random();
    let obj3 = ObjectID::random();

    let claim = TransactionClaim::ImmutableInputObjects(vec![obj1, obj2, obj3]);

    match &claim {
        TransactionClaim::ImmutableInputObjects(objs) => {
            assert_eq!(objs.len(), 3);
            assert!(objs.contains(&obj1));
            assert!(objs.contains(&obj2));
            assert!(objs.contains(&obj3));
        }
        _ => panic!("Expected ImmutableInputObjects claim"),
    }
}

#[test]
fn test_transaction_with_claims_from_aliases() {
    let aliases = nonempty![(0u8, Some(SequenceNumber::from(5)))];

    let tx_with_claims = TransactionWithClaims::from_aliases("test_tx", aliases.clone());

    assert_eq!(*tx_with_claims.tx(), "test_tx");
    assert_eq!(tx_with_claims.aliases().unwrap().head.0, 0u8);
    assert_eq!(
        tx_with_claims.aliases().unwrap().head.1,
        Some(SequenceNumber::from(5))
    );
}

#[test]
fn test_transaction_with_claims_multiple_claims() {
    let aliases = nonempty![(0u8, None)];
    let immutable_objs = vec![ObjectID::random(), ObjectID::random()];

    let claims = vec![
        TransactionClaim::AddressAliasesV2(aliases.clone()),
        TransactionClaim::ImmutableInputObjects(immutable_objs.clone()),
    ];

    let tx_with_claims = TransactionWithClaims::new("test_tx", claims);

    // Should be able to get aliases
    assert_eq!(tx_with_claims.aliases().unwrap().head.0, 0u8);

    // Should be able to get immutable objects
    let retrieved_immutable = tx_with_claims.get_immutable_objects();
    assert_eq!(retrieved_immutable.len(), 2);
}

#[test]
fn test_transaction_with_claims_no_immutable_objects() {
    let aliases = nonempty![(0u8, None)];

    let tx_with_claims = TransactionWithClaims::from_aliases("test_tx", aliases);

    // Should have aliases but no immutable objects
    assert_eq!(tx_with_claims.aliases().unwrap().head.0, 0u8);
    assert!(tx_with_claims.get_immutable_objects().is_empty());
}

#[test]
fn test_transaction_claim_serialization() {
    let aliases = nonempty![(0u8, Some(SequenceNumber::from(1)))];

    let claim = TransactionClaim::AddressAliasesV2(aliases);

    let serialized = bcs::to_bytes(&claim).expect("serialization should succeed");
    let deserialized: TransactionClaim =
        bcs::from_bytes(&serialized).expect("deserialization should succeed");

    assert_eq!(claim, deserialized);
}

#[test]
fn test_transaction_claim_immutable_objects_serialization() {
    let objs = vec![ObjectID::random(), ObjectID::random()];
    let claim = TransactionClaim::ImmutableInputObjects(objs.clone());

    let serialized = bcs::to_bytes(&claim).expect("serialization should succeed");
    let deserialized: TransactionClaim =
        bcs::from_bytes(&serialized).expect("deserialization should succeed");

    assert_eq!(claim, deserialized);
}

#[test]
fn test_transaction_with_claims_empty_immutable_objects() {
    let aliases = nonempty![(0u8, None)];

    let claims = vec![
        TransactionClaim::AddressAliasesV2(aliases),
        TransactionClaim::ImmutableInputObjects(vec![]),
    ];

    let tx_with_claims = TransactionWithClaims::new("test_tx", claims);

    // Should return Some with empty vec
    let immutable = tx_with_claims.get_immutable_objects();
    assert!(immutable.is_empty());
}

#[test]
fn test_aliases_returns_none_when_not_present() {
    let claims = vec![TransactionClaim::ImmutableInputObjects(vec![
        ObjectID::random(),
    ])];
    let tx_with_claims = TransactionWithClaims::new("test_tx", claims);
    // aliases() should return None when AddressAliasesV2 claim is not present
    assert!(tx_with_claims.aliases().is_none());
}
