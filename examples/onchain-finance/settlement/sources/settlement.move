// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#settlement
module example::settlement;

use std::string::{Self, String};
use sui::coin::Coin;
use sui::event;

/// Hot potato (no `drop`): must be consumed in the same transaction.
public struct SettlementProof {
    recipient: address,
    amount: u64,
    coin_type: String,
}

public struct SettlementVerified has copy, drop {
    recipient: address,
    amount: u64,
}

/// Execute a payment and return a proof that must be consumed.
public fun pay_and_prove<T>(payment: Coin<T>, recipient: address): SettlementProof {
    let amount = payment.value();
    let coin_type = std::string::from_ascii(std::type_name::with_defining_ids<T>().into_string());
    transfer::public_transfer(payment, recipient);

    SettlementProof { recipient, amount, coin_type }
}

/// Consume the proof. Call this after verifying the payment details.
/// Because SettlementProof has no `drop`, this function MUST be called
/// in the same transaction. The proof cannot be ignored.
public fun verify_settlement(
    proof: SettlementProof,
    expected_recipient: address,
    expected_amount: u64,
) {
    assert!(proof.recipient == expected_recipient, 0);
    assert!(proof.amount >= expected_amount, 1);

    event::emit(SettlementVerified {
        recipient: proof.recipient,
        amount: proof.amount,
    });

    let SettlementProof { recipient: _, amount: _, coin_type: _ } = proof;
}
// docs::/#settlement
