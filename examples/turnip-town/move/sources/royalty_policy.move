// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A transfer policy for enforcing royalty fees for turnips.
module turnip_town::royalty_policy {
    use sui::coin::Coin;
    use sui::math;
    use sui::sui::SUI;
    use sui::transfer_policy::{
        Self as policy,
        TransferPolicy,
        TransferPolicyCap,
        TransferRequest,
    };

    // === Types ===
    public struct RULE() has drop;
    public struct Config() has store, drop;

    // === Constants ===

    /// 1% commission, in basis points
    const COMMISSION_BP: u16 = 1_00;

    /// Be paid at least 1 MIST for each transaction.
    const MIN_ROYALTY: u64 = 1;

    // === Errors ===

    /// Coin used for payment is not enough to cover the royalty.
    const EInsufficientAmount: u64 = 0;

    // === Public Functions ===

    /// Buyer action: pay the royalty.
    public fun pay<T: key + store>(
        policy: &mut TransferPolicy<T>,
        request: &mut TransferRequest<T>,
        payment: &mut Coin<SUI>,
        ctx: &mut TxContext,
    ) {
        let amount = (request.paid() as u128) * (COMMISSION_BP as u128) / 10_000;
        let amount = math::max(amount as u64, MIN_ROYALTY);

        assert!(payment.value() >= amount, EInsufficientAmount);
        let fee = payment.split(amount, ctx);
        policy::add_to_balance(RULE(), policy, fee);
        policy::add_receipt(RULE(), request)
    }

    // === Protected Functions ===

    /// Add the royalty policy to the given transfer policy.
    public(package) fun set<T: key + store>(
        policy: &mut TransferPolicy<T>,
        cap: &TransferPolicyCap<T>,
    ) {
        policy::add_rule(RULE(), policy, cap, Config());
    }
}
