// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#payments
module example::payments;

use sui::coin::Coin;
use sui::sui::SUI;

public struct PaymentConfig has key {
    id: UID,
    max_per_tx: u64,
    authorized_recipients: vector<address>,
}

public fun execute_payment(
    config: &PaymentConfig,
    payment: Coin<SUI>,
    recipient: address,
    ctx: &mut TxContext,
) {
    assert!(payment.value() <= config.max_per_tx, 0);
    assert!(config.authorized_recipients.contains(&recipient), 1);
    transfer::public_transfer(payment, recipient);
}
// docs::/payments
