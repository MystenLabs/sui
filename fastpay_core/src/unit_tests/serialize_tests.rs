// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::same_item_push)] // get_key_pair returns random elements

use super::*;
use crate::base_types::*;
use std::time::Instant;

#[test]
fn test_error() {
    let err = FastPayError::UnknownSigner;
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
    let req1 = AccountInfoRequest {
        sender: dbg_addr(0x20),
        request_sequence_number: None,
        request_received_transfers_excluding_first_nth: None,
    };
    let req2 = AccountInfoRequest {
        sender: dbg_addr(0x20),
        request_sequence_number: Some(SequenceNumber::from(129)),
        request_received_transfers_excluding_first_nth: None,
    };

    let buf1 = serialize_info_request(&req1);
    let buf2 = serialize_info_request(&req2);

    let result1 = deserialize_message(buf1.as_slice());
    let result2 = deserialize_message(buf2.as_slice());
    assert!(result1.is_ok());
    assert!(result2.is_ok());

    if let SerializedMessage::InfoReq(o) = result1.unwrap() {
        assert!(*o == req1);
    } else {
        panic!()
    }
    if let SerializedMessage::InfoReq(o) = result2.unwrap() {
        assert!(*o == req2);
    } else {
        panic!()
    }
}

#[test]
fn test_order() {
    let (sender_name, sender_key) = get_key_pair();

    let transfer = Transfer {
        sender: sender_name,
        recipient: Address::Primary(dbg_addr(0x20)),
        amount: Amount::from(5),
        sequence_number: SequenceNumber::new(),
        user_data: UserData::default(),
    };
    let transfer_order = TransferOrder::new(transfer, &sender_key);

    let buf = serialize_transfer_order(&transfer_order);
    let result = deserialize_message(buf.as_slice());
    assert!(result.is_ok());
    if let SerializedMessage::Order(o) = result.unwrap() {
        assert!(*o == transfer_order);
    } else {
        panic!()
    }

    let (sender_name, sender_key) = get_key_pair();
    let transfer2 = Transfer {
        sender: sender_name,
        recipient: Address::FastPay(dbg_addr(0x20)),
        amount: Amount::from(5),
        sequence_number: SequenceNumber::new(),
        user_data: UserData::default(),
    };
    let transfer_order2 = TransferOrder::new(transfer2, &sender_key);

    let buf = serialize_transfer_order(&transfer_order2);
    let result = deserialize_message(buf.as_slice());
    assert!(result.is_ok());
    if let SerializedMessage::Order(o) = result.unwrap() {
        assert!(*o == transfer_order2);
    } else {
        panic!()
    }
}

#[test]
fn test_vote() {
    let (sender_name, sender_key) = get_key_pair();
    let transfer = Transfer {
        sender: sender_name,
        recipient: Address::Primary(dbg_addr(0x20)),
        amount: Amount::from(5),
        sequence_number: SequenceNumber::new(),
        user_data: UserData::default(),
    };
    let order = TransferOrder::new(transfer, &sender_key);

    let (authority_name, authority_key) = get_key_pair();
    let vote = SignedTransferOrder::new(order, authority_name, &authority_key);

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
    let transfer = Transfer {
        sender: sender_name,
        recipient: Address::Primary(dbg_addr(0x20)),
        amount: Amount::from(5),
        sequence_number: SequenceNumber::new(),
        user_data: UserData::default(),
    };
    let order = TransferOrder::new(transfer, &sender_key);
    let mut cert = CertifiedTransferOrder {
        value: order,
        signatures: Vec::new(),
    };

    for _ in 0..3 {
        let (authority_name, authority_key) = get_key_pair();
        let sig = Signature::new(&cert.value.transfer, &authority_key);

        cert.signatures.push((authority_name, sig));
    }

    let buf = serialize_cert(&cert);
    let result = deserialize_message(buf.as_slice());
    assert!(result.is_ok());
    if let SerializedMessage::Cert(o) = result.unwrap() {
        assert!(*o == cert);
    } else {
        panic!()
    }
}

#[test]
fn test_info_response() {
    let (sender_name, sender_key) = get_key_pair();
    let transfer = Transfer {
        sender: sender_name,
        recipient: Address::Primary(dbg_addr(0x20)),
        amount: Amount::from(5),
        sequence_number: SequenceNumber::new(),
        user_data: UserData::default(),
    };
    let order = TransferOrder::new(transfer, &sender_key);

    let (auth_name, auth_key) = get_key_pair();
    let vote = SignedTransferOrder::new(order.clone(), auth_name, &auth_key);

    let mut cert = CertifiedTransferOrder {
        value: order,
        signatures: Vec::new(),
    };

    for _ in 0..3 {
        let (authority_name, authority_key) = get_key_pair();
        let sig = Signature::new(&cert.value.transfer, &authority_key);

        cert.signatures.push((authority_name, sig));
    }

    let resp1 = AccountInfoResponse {
        sender: dbg_addr(0x20),
        balance: Balance::from(50),
        next_sequence_number: SequenceNumber::new(),
        pending_confirmation: None,
        requested_certificate: None,
        requested_received_transfers: Vec::new(),
    };
    let resp2 = AccountInfoResponse {
        sender: dbg_addr(0x20),
        balance: Balance::from(50),
        next_sequence_number: SequenceNumber::new(),
        pending_confirmation: Some(vote.clone()),
        requested_certificate: None,
        requested_received_transfers: Vec::new(),
    };
    let resp3 = AccountInfoResponse {
        sender: dbg_addr(0x20),
        balance: Balance::from(50),
        next_sequence_number: SequenceNumber::new(),
        pending_confirmation: None,
        requested_certificate: Some(cert.clone()),
        requested_received_transfers: Vec::new(),
    };
    let resp4 = AccountInfoResponse {
        sender: dbg_addr(0x20),
        balance: Balance::from(50),
        next_sequence_number: SequenceNumber::new(),
        pending_confirmation: Some(vote),
        requested_certificate: Some(cert),
        requested_received_transfers: Vec::new(),
    };

    for resp in [resp1, resp2, resp3, resp4].iter() {
        let buf = serialize_info_response(resp);
        let result = deserialize_message(buf.as_slice());
        assert!(result.is_ok());
        if let SerializedMessage::InfoResp(o) = result.unwrap() {
            assert!(*o == *resp);
        } else {
            panic!()
        }
    }
}

#[test]
fn test_time_order() {
    let (sender_name, sender_key) = get_key_pair();
    let transfer = Transfer {
        sender: sender_name,
        recipient: Address::Primary(dbg_addr(0x20)),
        amount: Amount::from(5),
        sequence_number: SequenceNumber::new(),
        user_data: UserData::default(),
    };

    let mut buf = Vec::new();
    let now = Instant::now();
    for _ in 0..100 {
        let transfer_order = TransferOrder::new(transfer.clone(), &sender_key);
        serialize_transfer_order_into(&mut buf, &transfer_order).unwrap();
    }
    println!("Write Order: {} microsec", now.elapsed().as_micros() / 100);

    let mut buf2 = buf.as_slice();
    let now = Instant::now();
    for _ in 0..100 {
        if let SerializedMessage::Order(order) = deserialize_message(&mut buf2).unwrap() {
            order.check_signature().unwrap();
        }
    }
    assert!(deserialize_message(&mut buf2).is_err());
    println!(
        "Read & Check Order: {} microsec",
        now.elapsed().as_micros() / 100
    );
}

#[test]
fn test_time_vote() {
    let (sender_name, sender_key) = get_key_pair();
    let transfer = Transfer {
        sender: sender_name,
        recipient: Address::Primary(dbg_addr(0x20)),
        amount: Amount::from(5),
        sequence_number: SequenceNumber::new(),
        user_data: UserData::default(),
    };
    let order = TransferOrder::new(transfer, &sender_key);

    let (authority_name, authority_key) = get_key_pair();

    let mut buf = Vec::new();
    let now = Instant::now();
    for _ in 0..100 {
        let vote = SignedTransferOrder::new(order.clone(), authority_name, &authority_key);
        serialize_vote_into(&mut buf, &vote).unwrap();
    }
    println!("Write Vote: {} microsec", now.elapsed().as_micros() / 100);

    let mut buf2 = buf.as_slice();
    let now = Instant::now();
    for _ in 0..100 {
        if let SerializedMessage::Vote(vote) = deserialize_message(&mut buf2).unwrap() {
            vote.signature
                .check(&vote.value.transfer, vote.authority)
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
    let transfer = Transfer {
        sender: sender_name,
        recipient: Address::Primary(dbg_addr(0)),
        amount: Amount::from(5),
        sequence_number: SequenceNumber::new(),
        user_data: UserData::default(),
    };
    let order = TransferOrder::new(transfer, &sender_key);
    let mut cert = CertifiedTransferOrder {
        value: order,
        signatures: Vec::new(),
    };

    for _ in 0..7 {
        let (authority_name, authority_key) = get_key_pair();
        let sig = Signature::new(&cert.value.transfer, &authority_key);
        cert.signatures.push((authority_name, sig));
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
            Signature::verify_batch(&cert.value.transfer, &cert.signatures).unwrap();
        }
    }
    assert!(deserialize_message(buf2).is_err());
    println!(
        "Read & Quickcheck Cert: {} microsec",
        now.elapsed().as_micros() / count
    );
}
