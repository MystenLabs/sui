// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::collections::BTreeMap;

#[test]
fn test_signed_values() {
    let mut authorities = BTreeMap::new();
    let (a1, sec1) = get_key_pair();
    let (a2, sec2) = get_key_pair();
    let (a3, sec3) = get_key_pair();

    authorities.insert(/* address */ a1, /* voting right */ 1);
    authorities.insert(/* address */ a2, /* voting right */ 0);
    let committee = Committee::new(authorities);

    let transfer = Transfer {
        sender: a1,
        recipient: Address::FastPay(a2),
        amount: Amount::from(1),
        sequence_number: SequenceNumber::new(),
        user_data: UserData::default(),
    };
    let order = TransferOrder::new(transfer.clone(), &sec1);
    let bad_order = TransferOrder::new(transfer, &sec2);

    let v = SignedTransferOrder::new(order.clone(), a1, &sec1);
    assert!(v.check(&committee).is_ok());

    let v = SignedTransferOrder::new(order.clone(), a2, &sec2);
    assert!(v.check(&committee).is_err());

    let v = SignedTransferOrder::new(order, a3, &sec3);
    assert!(v.check(&committee).is_err());

    let v = SignedTransferOrder::new(bad_order, a1, &sec1);
    assert!(v.check(&committee).is_err());
}

#[test]
fn test_certificates() {
    let (a1, sec1) = get_key_pair();
    let (a2, sec2) = get_key_pair();
    let (a3, sec3) = get_key_pair();

    let mut authorities = BTreeMap::new();
    authorities.insert(/* address */ a1, /* voting right */ 1);
    authorities.insert(/* address */ a2, /* voting right */ 1);
    let committee = Committee::new(authorities);

    let transfer = Transfer {
        sender: a1,
        recipient: Address::FastPay(a2),
        amount: Amount::from(1),
        sequence_number: SequenceNumber::new(),
        user_data: UserData::default(),
    };
    let order = TransferOrder::new(transfer.clone(), &sec1);
    let bad_order = TransferOrder::new(transfer, &sec2);

    let v1 = SignedTransferOrder::new(order.clone(), a1, &sec1);
    let v2 = SignedTransferOrder::new(order.clone(), a2, &sec2);
    let v3 = SignedTransferOrder::new(order.clone(), a3, &sec3);

    let mut builder = SignatureAggregator::try_new(order.clone(), &committee).unwrap();
    assert!(builder
        .append(v1.authority, v1.signature)
        .unwrap()
        .is_none());
    let mut c = builder.append(v2.authority, v2.signature).unwrap().unwrap();
    assert!(c.check(&committee).is_ok());
    c.signatures.pop();
    assert!(c.check(&committee).is_err());

    let mut builder = SignatureAggregator::try_new(order, &committee).unwrap();
    assert!(builder
        .append(v1.authority, v1.signature)
        .unwrap()
        .is_none());
    assert!(builder.append(v3.authority, v3.signature).is_err());

    assert!(SignatureAggregator::try_new(bad_order, &committee).is_err());
}
