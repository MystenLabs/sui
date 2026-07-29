// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::settlement;

use std::string::String;
use sui::coin::Coin;
use sui::event;

#[error]
const EPaymentTooSmall: vector<u8> = b"Payment amount is less than the required amount";

public struct PaymentSettled has copy, drop {
    recipient: address,
    amount: u64,
    required_amount: u64,
    coin_type: String,
}

/// Hot potato (no `drop`): must be consumed in the same transaction.
/// Forces the caller to acknowledge the payment before the PTB ends.
public struct SettlementProof {
    recipient: address,
    amount: u64,
    required_amount: u64,
}

// docs::#settlement
/// Execute a payment, emit a settlement event, and return a proof
/// that must be consumed in the same transaction (hot potato).
public fun pay_and_prove<T>(
    payment: Coin<T>,
    recipient: address,
    required_amount: u64,
): SettlementProof {
    let amount = payment.value();
    assert!(amount >= required_amount, EPaymentTooSmall);
    let coin_type = std::type_name::with_defining_ids<T>().into_string().to_string();
    transfer::public_transfer(payment, recipient);

    event::emit(PaymentSettled { recipient, amount, required_amount, coin_type });

    SettlementProof { recipient, amount, required_amount }
}

/// Consume the proof. Because SettlementProof has no `drop`,
/// this function MUST be called in the same transaction.
public fun consume_proof(proof: SettlementProof) {
    let SettlementProof { .. } = proof;
}
// docs::/#settlement
