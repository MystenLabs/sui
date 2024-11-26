// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Note that there isn't any cash register for this and while the code is
/// small it hides the complexity elsewhere. In particular-- the authentication
/// for the address is all held behind a 1-of-N multisig where each authorized user has a key.
/// This would present issues either in the case where the register is
/// compromised (either via a key being exposed or an authorized user leaving the
/// company).
///
/// In either case a new address would need to be created, and customers would
/// then need to understand that they should interact with the new address and
/// not the old (now possibly compomised) one.
///
/// Overall, while it may seem simple it has pretty significant shortcomings
/// that can be overcome with a transfer-to-object  based approach using a
/// shared-object register for tracking authorization.
module owned_no_tto::cash_register;

use common::identified_payment::{Self, IdentifiedPayment};
use sui::{coin::{Self, Coin}, event, sui::SUI};

public struct PaymentProcessed has copy, drop { payment_id: u64, amount: u64 }

public fun process_payment(payment: IdentifiedPayment): Coin<SUI> {
    let (payment_id, coin) = identified_payment::unpack(payment);
    event::emit(PaymentProcessed { payment_id, amount: coin::value(&coin) });
    coin
}

// NB: Payments are performed with the `identified_payment::make_payment` function.
