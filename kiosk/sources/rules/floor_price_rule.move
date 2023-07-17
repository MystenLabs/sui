// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Description:
/// This module defines a Rule which sets the floor price for items of type T.
///
/// Configuration:
/// - floor_price - the floor price in MIST.
///
/// Use cases:
/// - Defining a floor price for all trades of type T.
/// - Prevent trading of locked items with low amounts (e.g. by using purchase_cap).
/// 
module kiosk::floor_price_rule {
    use sui::transfer_policy::{
        Self as policy,
        TransferPolicy,
        TransferPolicyCap,
        TransferRequest
    };

    /// The price was lower than the floor price.
    const EPriceTooSmall: u64 = 0;

    /// The "Rule" witness to authorize the policy.
    struct Rule has drop {}

    /// Configuration for the `Floor Price Rule`.
    /// It holds the minimum price that an item can be sold at.
    /// There can't be any sales with a price < than the floor_price.
    struct Config has store, drop {
        floor_price: u64
    }

    /// Creator action: Add the Floor Price Rule for the `T`.
    /// Pass in the `TransferPolicy`, `TransferPolicyCap` and `floor_price`.
    public fun add<T: key + store>(
        policy: &mut TransferPolicy<T>,
        cap: &TransferPolicyCap<T>,
        floor_price: u64
    ) {
        policy::add_rule(Rule {}, policy, cap, Config { floor_price })
    }

    /// Buyer action: Prove that the amount is higher or equal to the floor_price.
    public fun prove<T: key + store>(
        policy: &mut TransferPolicy<T>,
        request: &mut TransferRequest<T>
    ) {
        let config: &Config = policy::get_rule(Rule {}, policy);

        assert!(policy::paid(request) >= config.floor_price, EPriceTooSmall);
        
        policy::add_receipt(Rule {}, request)
    }
}
