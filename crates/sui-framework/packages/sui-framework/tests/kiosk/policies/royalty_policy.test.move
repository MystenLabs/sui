// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// A `TransferPolicy` Rule which implements percentage-based royalty fee.
module sui::royalty_policy {
    use sui::sui::SUI;
    use sui::coin::{Self, Coin};
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
    public struct Rule has drop {}

    /// Configuration for the Rule.
    public struct Config has store, drop {
        amount_bp: u16
    }

    /// Creator action: Set the Royalty policy for the `T`.
    public fun set<T: key + store>(
        policy: &mut TransferPolicy<T>,
        cap: &TransferPolicyCap<T>,
        amount_bp: u16
    ) {
        assert!(amount_bp < MAX_BPS, EIncorrectArgument);
        policy::add_rule(Rule {}, policy, cap, Config { amount_bp })
    }

    /// Buyer action: Pay the royalty fee for the transfer.
    public fun pay<T: key + store>(
        policy: &mut TransferPolicy<T>,
        request: &mut TransferRequest<T>,
        payment: &mut Coin<SUI>,
        ctx: &mut TxContext
    ) {
        let config: &Config = policy::get_rule(Rule {}, policy);
        let paid = policy::paid(request);
        let amount = (paid as u128 * (config.amount_bp as u128) / 10_000) as u64;

        assert!(coin::value(payment) >= amount, EInsufficientAmount);

        let fee = coin::split(payment, amount, ctx);
        policy::add_to_balance(Rule {}, policy, fee);
        policy::add_receipt(Rule {}, request)
    }
}

#[test_only]
module sui::royalty_policy_tests {
    use sui::coin;
    use sui::sui::SUI;
    use sui::royalty_policy;
    use sui::transfer_policy as policy;
    use sui::transfer_policy_tests as test;

    #[test]
    fun test_default_flow() {
        let ctx = &mut tx_context::dummy();
        let (mut policy, cap) = test::prepare(ctx);

        // 1% royalty
        royalty_policy::set(&mut policy, &cap, 100);

        let mut request = policy::new_request(test::fresh_id(ctx), 100_000, test::fresh_id(ctx));
        let mut payment = coin::mint_for_testing<SUI>(2000, ctx);

        royalty_policy::pay(&mut policy, &mut request, &mut payment, ctx);
        policy::confirm_request(&policy, request);

        let remainder = coin::burn_for_testing(payment);
        let profits = test::wrapup(policy, cap, ctx);

        assert!(remainder == 1000);
        assert!(profits == 1000);
    }

    #[test]
    #[expected_failure(abort_code = sui::royalty_policy::EIncorrectArgument)]
    fun test_incorrect_config() {
        let ctx = &mut tx_context::dummy();
        let (mut policy, cap) = test::prepare(ctx);

        royalty_policy::set(&mut policy, &cap, 11_000);
        test::wrapup(policy, cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::royalty_policy::EInsufficientAmount)]
    fun test_insufficient_amount() {
        let ctx = &mut tx_context::dummy();
        let (mut policy, cap) = test::prepare(ctx);

        // 1% royalty
        royalty_policy::set(&mut policy, &cap, 100);

        // Requires 1_000 MIST, coin has only 999
        let mut request = policy::new_request(test::fresh_id(ctx), 100_000, test::fresh_id(ctx));
        let mut payment = coin::mint_for_testing<SUI>(999, ctx);

        royalty_policy::pay(&mut policy, &mut request, &mut payment, ctx);
        policy::confirm_request(&policy, request);

        coin::burn_for_testing(payment);
        test::wrapup(policy, cap, ctx);
    }
}
