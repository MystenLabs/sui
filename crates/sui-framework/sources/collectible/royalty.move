// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This module implements a set of basic primitives for NftSave's
/// Transfer Policies to improve discoverability and serve as a
/// base for building on top.
module sui::royalty {
    use std::option::{Self, Option};
    use sui::nft_safe::{Self, TransferRequest, TransferPolicy, TransferCap};
    use sui::tx_context::TxContext;
    use sui::balance::{Self, Balance};
    use sui::object::{Self, UID, ID};
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use sui::event;

    /// Utility constant to calculate the percentage of price to require.
    const MAX_AMOUNT: u16 = 10_000;

    /// For when trying to create a new RoyaltyPolicy with more than 100%.
    ///  Or when trying to withdraw more than stored `balance`.
    const EIncorrectAmount: u64 = 0;

    /// A transfer policy for a single type `T` which collects a certain
    /// fee from the `nft_safe` deals and stores them for the policy issuer.
    struct RoyaltyPolicy<phantom T: key + store> has key, store {
        id: UID,
        /// The `TransferCap` for the `T` which is used to call
        /// the `nft_safe::allow_transfer` and allow the trade.
        cap: TransferCap<T>,
        /// Percentage of the trade amount which is required for the
        /// transfer approval. Denominated in basis points.
        /// - 10_000 = 100%
        /// - 100 = 1%
        /// - 1 = 0.01%
        amount_bp: u16,
        /// Accumulated balance - the owner of the Policy can withdraw
        /// at any time.
        balance: Balance<SUI>,
        /// Store cap ID for visibility and discoverability purposes
        royalty_collector_cap: ID,
    }

    /// A Capability that grants the bearer the ability to change the amount of
    /// royalties collected as well as to withdraw from the `policy.balance`.
    struct RoyaltyCollectorCap<phantom T: key + store> has key, store {
        id: UID,
        /// Purely cosmetic and discovery field.
        /// There should be only one Policy for the type `T` (although it
        /// is not enforced anywhere by default).
        policy_id: ID
    }

    // === Events ===

    /// Event: fired when a new policy has been created for the type `T`. Meaning
    /// that in most of the cases where a `RoyaltyPolicy` is a shared object, it
    /// can be used in the `sui::royalty::pay` function.
    struct PolicyCreated<phantom T: key + store> has copy, drop {
        id: ID,
    }

    // === Public / Everyone ===

    /// Perform a Royalty payment and signs the transfer.
    /// 
    /// The hot potato transfer request object now has an extra signature.
    /// Its `TransferPolicy<T>` defines how many signatures are necessary to
    /// finalize the trade.
    public fun pay_and_sign<T: key + store>(
        policy: &mut RoyaltyPolicy<T>,
        transfer_request: TransferRequest<T>,
        coin: &mut Coin<SUI>
    ): TransferRequest<T> {
        let paid = nft_safe::transfer_request_paid(&transfer_request);
        nft_safe::sign_transfer(&policy.cap, &mut transfer_request);
        let amount = (((paid as u128) * (policy.amount_bp as u128) / (MAX_AMOUNT as u128)) as u64);

        let royalty_payment = balance::split(coin::balance_mut(coin), amount);
        balance::join(&mut policy.balance, royalty_payment);

        transfer_request
    }

    /// Perform a Royalty payment and tries to destroy the hot potato.
    /// 
    /// Aborts if there are not enough signatures on the transfer cap.
    public fun pay<T: key + store>(
        transfer_policy: &TransferPolicy<T>,
        royalty_policy: &mut RoyaltyPolicy<T>,
        transfer_request: TransferRequest<T>,
        coin: &mut Coin<SUI>
    ) {
        let transfer_request = pay_and_sign(royalty_policy, transfer_request, coin);
        nft_safe::allow_transfer(transfer_policy, transfer_request);
    }

    // === Creator only ===

    /// Create new `RoyaltyPolicy` for the `T` and require an `amount`
    /// percentage of the trade amount for the transfer to be approved.
    public fun new_royalty_policy<T: key + store>(
        cap: TransferCap<T>,
        amount_bp: u16,
        ctx: &mut TxContext
    ): (RoyaltyPolicy<T>, RoyaltyCollectorCap<T>) {
        assert!(amount_bp <= MAX_AMOUNT && amount_bp != 0, EIncorrectAmount);

        let royalty_collector_cap_uid = object::new(ctx);
        let policy = RoyaltyPolicy {
            cap, amount_bp,
            id: object::new(ctx),
            balance: balance::zero(),
            royalty_collector_cap: object::uid_to_inner(&royalty_collector_cap_uid)
        };
        let id = object::id(&policy);
        let cap = RoyaltyCollectorCap {
            id: royalty_collector_cap_uid,
            policy_id: id
        };

        event::emit(PolicyCreated<T> { id });

        (policy, cap)
    }

    /// Change the amount in the `RoyaltyPolicy`.
    public fun set_amount<T: key + store>(
        policy: &mut RoyaltyPolicy<T>,
        _cap: &RoyaltyCollectorCap<T>,
        amount: u16,
    ) {
        assert!(amount > 0 && amount <= MAX_AMOUNT, EIncorrectAmount);
        policy.amount_bp = amount
    }

    /// Withdraw `amount` of SUI from the `policy.balance`.
    public fun withdraw<T: key + store>(
        policy: &mut RoyaltyPolicy<T>,
        _cap: &RoyaltyCollectorCap<T>,
        amount: Option<u64>,
        ctx: &mut TxContext
    ): Coin<SUI> {
        let available = balance::value(&policy.balance);
        let amount = if (option::is_some(&amount)) {
            option::destroy_some(amount)
        } else {
            available
        };

        assert!(amount <= available, EIncorrectAmount);
        coin::take(&mut policy.balance, amount, ctx)
    }

    /// Unpack a RoyaltyPolicy object if it is not shared (!!!) and
    /// return the `TransferCap` and the remaining balance.
    public fun destroy_and_withdraw<T: key + store>(
        policy: RoyaltyPolicy<T>,
        cap: RoyaltyCollectorCap<T>,
        ctx: &mut TxContext
    ): (TransferCap<T>, Coin<SUI>) {
        let RoyaltyPolicy {
            id, amount_bp: _, royalty_collector_cap: _, cap: transfer_cap, balance
        } = policy;
        let RoyaltyCollectorCap { id: cap_id, policy_id: _ } = cap;

        object::delete(cap_id);
        object::delete(id);

        (transfer_cap, coin::from_balance(balance, ctx))
    }

    // === Field access ===

    /// Get the `amount` field.
    public fun amount<T: key + store>(self: &RoyaltyPolicy<T>): u16 {
        self.amount_bp
    }

    /// Get the `balance` field.
    public fun balance<T: key + store>(self: &RoyaltyPolicy<T>): u64 {
        balance::value(&self.balance)
    }
}
