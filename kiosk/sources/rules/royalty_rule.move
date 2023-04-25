// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Description:
/// This module defines a Rule which requires a payment on a purchase.
/// The payment amount can be either a fixed amount (min_amount) or a
/// percentage of the purchase price (amount_bp). Or both: the higher
/// of the two is used.
///
/// Configuration:
/// - amount_bp - the percentage of the purchase price to be paid as a
/// fee, denominated in basis points (100_00 = 100%, 1 = 0.01%).
/// - min_amount - the minimum amount to be paid as a fee if the relative
/// amount is lower than this setting.
///
/// Use cases:
/// - Percentage-based Royalty fee for the creator of the NFT.
/// - Fixed commission fee on a trade.
/// - A mix of both: the higher of the two is used.
///
/// Notes:
/// - To use it as a fixed commission set the `amount_bp` to 0 and use the
/// `min_amount` to set the fixed amount.
/// - To use it as a percentage-based fee set the `min_amount` to 0 and use
/// the `amount_bp` to set the percentage.
/// - To use it as a mix of both set the `min_amount` to the min amount
/// acceptable and the `amount_bp` to the percentage of the purchase price.
/// The higher of the two will be used.
///
module kiosk::royalty_rule {
    use sui::sui::SUI;
    use sui::coin::{Self, Coin};
    use sui::tx_context::TxContext;
    use sui::transfer_policy::{
        Self as policy,
        TransferPolicy,
        TransferPolicyCap,
        TransferRequest
    };

    /// The `amount_bp` passed is more than 100%.
    const EIncorrectArgument: u64 = 0;
    /// The `Coin` used for payment is not enough to cover the fee.
    const EInsufficientAmount: u64 = 1;

    /// Max value for the `amount_bp`.
    const MAX_BPS: u16 = 10_000;

    /// The "Rule" witness to authorize the policy.
    struct Rule has drop {}

    /// Configuration for the Rule. The `amount_bp` is the percentage
    /// of the transfer amount to be paid as a royalty fee. The `min_amount`
    /// is the minimum amount to be paid if the percentage based fee is
    /// lower than the `min_amount` setting.
    ///
    /// Adding a mininum amount is useful to enforce a fixed fee even if
    /// the transfer amount is very small or 0.
    struct Config has store, drop {
        amount_bp: u16,
        min_amount: u64
    }

    /// Creator action: Add the Royalty Rule for the `T`.
    /// Pass in the `TransferPolicy`, `TransferPolicyCap` and the configuration
    /// for the policy: `amount_bp` and `min_amount`.
    public fun add<T: key + store>(
        policy: &mut TransferPolicy<T>,
        cap: &TransferPolicyCap<T>,
        amount_bp: u16,
        min_amount: u64
    ) {
        assert!(amount_bp <= MAX_BPS, EIncorrectArgument);
        policy::add_rule(Rule {}, policy, cap, Config { amount_bp, min_amount })
    }

    /// Buyer action: Pay the royalty fee for the transfer.
    public fun pay<T: key + store>(
        policy: &mut TransferPolicy<T>,
        request: &mut TransferRequest<T>,
        payment: &mut Coin<SUI>,
        ctx: &mut TxContext
    ) {
        let paid = policy::paid(request);
        let amount = fee_amount(policy, paid);

        assert!(coin::value(payment) >= amount, EInsufficientAmount);

        let fee = coin::split(payment, amount, ctx);
        policy::add_to_balance(Rule {}, policy, fee);
        policy::add_receipt(Rule {}, request)
    }

    /// Helper function to calculate the amount to be paid for the transfer.
    /// Can be used dry-runned to estimate the fee amount based on the Kiosk listing price.
    public fun fee_amount<T: key + store>(policy: &TransferPolicy<T>, paid: u64): u64 {
        let config: &Config = policy::get_rule(Rule {}, policy);
        let amount = (((paid as u128) * (config.amount_bp as u128) / 10_000) as u64);

        // If the amount is less than the minimum, use the minimum
        if (amount < config.min_amount) {
            amount = config.min_amount
        };

        amount
    }
}
