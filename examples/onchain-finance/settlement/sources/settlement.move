// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::settlement;

use std::string::String;
use sui::coin::Coin;
use sui::event;

public struct PaymentSettled has copy, drop {
    recipient: address,
    amount: u64,
    coin_type: String,
}

/// Hot potato (no `drop`): must be consumed in the same transaction.
/// Forces the caller to acknowledge the payment before the PTB ends.
public struct SettlementReceipt {
    recipient: address,
    amount: u64,
}

// docs::#settlement
/// Execute a payment, emit a settlement event, and return a receipt
/// that must be consumed in the same transaction (hot potato).
public fun pay_and_prove<T>(payment: Coin<T>, recipient: address): SettlementReceipt {
    let amount = payment.value();
    let coin_type = std::type_name::get_with_original_ids<T>().into_string().to_string();
    transfer::public_transfer(payment, recipient);

    event::emit(PaymentSettled { recipient, amount, coin_type });

    SettlementReceipt { recipient, amount }
}

/// Consume the receipt. Because SettlementReceipt has no `drop`,
/// this function MUST be called in the same transaction.
public fun consume_receipt(receipt: SettlementReceipt) {
    let SettlementReceipt { .. } = receipt;
}
// docs::/#settlement
