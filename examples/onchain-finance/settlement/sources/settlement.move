// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::settlement;

use std::string::String;
use sui::coin::Coin;
use sui::event;

#[error]
const ERecipientMismatch: vector<u8> = b"Recipient does not match proof";
#[error]
const EInsufficientAmount: vector<u8> = b"Amount is less than expected";

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

// docs::#settlement
/// Execute a payment and return a proof that must be consumed.
public fun pay_and_prove<T>(payment: Coin<T>, recipient: address): SettlementProof {
    let amount = payment.value();
    let coin_type = std::type_name::get_with_original_ids<T>().into_string().to_string();
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
    let SettlementProof { recipient, amount, coin_type: _ } = proof;
    assert!(recipient == expected_recipient, ERecipientMismatch);
    assert!(amount >= expected_amount, EInsufficientAmount);

    event::emit(SettlementVerified { recipient, amount });
}
// docs::/#settlement
