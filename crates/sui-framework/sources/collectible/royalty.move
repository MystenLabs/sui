// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This module implements a set of basic primitives for Kiosk's
/// Transfer Policies to improve discoverability and serve as a
/// base for building on top.
module sui::royalty {
    use sui::sui::SUI;
    use sui::transfer;
    use sui::coin::{Self, Coin};
    use sui::tx_context::TxContext;
    use sui::object::{Self, UID};
    use sui::balance::{Self, Balance};
    use sui::kiosk::{Self, TransferRequest, TransferPolicyCap};

    /// Utility constant to calculate the percentage of price to require.
    const MAX_AMOUNT: u16 = 10_000;

    /// For when trying to create a new RoyaltyPolicy with more than 100%.
    const EIncorrectAmount: u64 = 0;

    /// A transfer policy for a single type `T` which collects a certain
    /// fee from the `kiosk` deals and stores them for policy issuer.
    struct RoyaltyPolicy<phantom T: key + store> has key, store {
        id: UID,
        /// The `TransferPolicyCap` for the `T` which is used to call
        /// the `kiosk::allow_transfer` and allow the trade.
        cap: TransferPolicyCap<T>,
        /// Percentage of the trade amount which is required for the
        /// transfer approval. Denominated in basis points.
        /// - 10_000 = 100%
        /// - 100 = 1%
        /// - 1 = 0.01%
        amount: u16,
        /// Accumulated balance - the owner of the Policy can withdraw
        /// at any time.
        balance: Balance<SUI>
    }

    /// A special function used to explicitly indicate that all of the
    /// trades can be performed freely. To achieve that, the `TransferPolicyCap`
    /// is immutably shared making it available for free use by anyone on the network.
    entry public fun set_zero_policy<T: key + store>(cap: TransferPolicyCap<T>) {
        // TODO: emit event
        transfer::freeze_object(cap)
    }

    /// Create new `RoyaltyPolicy` for the `T` and require an `amount`
    /// percentage of the trade amount for the transfer to be approved.
    public fun new_royalty_policy<T: key + store>(
        cap: TransferPolicyCap<T>,
        amount: u16,
        ctx: &mut TxContext
    ): RoyaltyPolicy<T> {
        assert!(amount <= MAX_AMOUNT, EIncorrectAmount);

        let id = object::new(ctx);

        RoyaltyPolicy {
            id,
            cap,
            amount,
            balance: balance::zero()
        }
    }

    /// Perform a Royalty payment and unblock the transfer by consuming
    /// the `TransferRequest` "hot potato". The `T` here type-locks the
    /// RoyaltyPolicy and TransferRequest making it impossible to call this
    /// function on a wrong type.
    public fun pay<T: key + store>(
        policy: &mut RoyaltyPolicy<T>,
        transfer_request: TransferRequest<T>,
        coin: &mut Coin<SUI>
    ) {
        let (paid, _from) = kiosk::allow_transfer(&policy.cap, transfer_request);
        let amount = (((paid as u128) * (policy.amount as u128) / (MAX_AMOUNT as u128)) as u64);
        
        let royalty_payment = balance::split(coin::balance_mut(coin), amount);
        balance::join(&mut policy.balance, royalty_payment);
    }
}
