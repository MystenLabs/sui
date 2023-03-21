// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This example introduces a `Subscription` type - a `Capability`-like object
/// with a limited number of uses. Once all of them are used, the subscription can
/// be renewed or destroyed.
///
/// Design of this module implies that an application will implement the subscription
/// interface using a `Witness` pattern, while some features are given by default
/// and do not require implementation.
///
///
/// The problem that illustrates usage of this module might be common in Sui. To
/// get to it, we need to provide some background:
///
/// - Shared object transactions in Sui require consensus; and for each shared
/// object there's a sequence of transactions which use / reference it.
///
/// - Mixing multiple shared objects within the same transaction potentially leads
/// to all objects being slowed down depending on how loaded they are.
///
/// - The only way to effectively use multiple shared objects is to be able to call
/// a function in Move which makes use of them. And these actions require `public`
/// function visibility.
///
/// Case in this example is a mocked up liquidity pool implementation with two
/// functions: `swap` - available only directly because of the `entry` visibility
/// and a `dev_swap` - doing the same thing but can be used on chain (ie for
/// cross-chain swaps), and can be used in a function with other shared objects.
///
/// The former is free because this functionality is meant to be available to
/// everyone. While the latter gives additional profits to the liquidity pool by
/// charging extensive (and potentially slowing) usage.
///
module defi::dev_pass {
    use sui::tx_context::{TxContext};
    use sui::object::{Self, UID};
    use sui::transfer;

    /// For when Subscription object no longer has uses.
    const ENoUses: u64 = 0;

    /// Owned object from which SingleUses are spawn.
    struct Subscription<phantom T> has key {
        id: UID,
        uses: u64
    }

    /// A single use potato to authorize actions.
    struct SingleUse<phantom T> {}

    // ======== Default Functions ========

    /// Public view for the `Subscription`.`uses` field.
    public fun uses<T>(s: &Subscription<T>): u64 { s.uses }

    /// If `Subscription` is owned, create `SingleUse` (hot potato) to use in the service.
    public fun use_pass<T>(s: &mut Subscription<T>): SingleUse<T> {
        assert!(s.uses != 0, ENoUses);
        s.uses = s.uses - 1;
        SingleUse {}
    }

    /// Burn a subscription without checking for number of uses. Allows Sui storage refunds
    /// when subscription is no longer needed.
    entry public fun destroy<T>(s: Subscription<T>) {
        let Subscription { id, uses: _ } = s;
        object::delete(id);
    }

    // ======== Implementable Functions ========

    /// Function to issue new `Subscription` with a specified number of uses.
    /// Implementable by an external module with a witness parameter T. Number of
    /// uses is determined by the actual implementation.
    public fun issue_subscription<T: drop>(_w: T, uses: u64, ctx: &mut TxContext): Subscription<T> {
        Subscription {
            id: object::new(ctx),
            uses
        }
    }

    /// Increase number of uses in the subscription.
    /// Implementable by an external module with a witness parameter T.
    public fun add_uses<T: drop>(_w: T, s: &mut Subscription<T>, uses: u64) {
        s.uses = s.uses + uses;
    }

    /// Confirm a use of a pass. Verified by the module that implements "Subscription API".
    /// Implementable by an external module with a witness parameter T. Confirmation is only
    /// available if the third party implements it and recognizes the use.
    public fun confirm_use<T: drop>(_w: T, pass: SingleUse<T>) {
        let SingleUse { } = pass;
    }

    /// Allow applications customize transferability of the `Subscription`.
    /// Implementable by an external module with a witness parameter T. Module can define whether
    /// a `Subscription` can be transferred to another account or not. Omitting this implementation
    /// will mean that the `Subscription` can not be transferred.
    public fun transfer<T: drop>(_w: T, s: Subscription<T>, to: address) {
        transfer::transfer(s, to)
    }
}

/// Rough outline of an AMM.
/// For simplicity pool implementation details are omitted but marked as comments to
/// show correlation with the `defi/pool.move` example.
module defi::some_amm {
    use defi::dev_pass::{Self, Subscription, SingleUse};
    use sui::tx_context::{Self, TxContext};

    /// A type to Mark subscription
    struct DEVPASS has drop {}
    /* Can be customized to: DEVPASS<phantom T, phantom S> to make one subscription per pool */
    /* And the price could be determined based on amount LP tokens issued or trade volume */

    /// Entry function that uses a shared pool object.
    /// Can only be accessed from outside of the chain - can't be called from another module.
    entry fun swap<T, S>(/* &mut Pool, Coin<T> ... */) { /* ... */ }

    /// Function similar to the `swap` but can be called from other Move modules.
    /// Opens up tooling; potentially slows down the AMM if multiple shared objects
    /// used in the same tx! And for that developers have to pay.
    public fun dev_swap<T, S>(p: SingleUse<DEVPASS>, /* &mut Pool, Coin<T> */): bool /* Coin<S> */ {
        dev_pass::confirm_use(DEVPASS {}, p);
        /* ... */
        true
    }

    /// Lastly there should a logic to purchase subscription. This AMM disallows transfers and
    /// issues Subscription object to the sender address.
    public fun purchase_pass(/* Coin<T> , */ ctx: &mut TxContext) {
        dev_pass::transfer(
            DEVPASS {},
            dev_pass::issue_subscription(DEVPASS {}, 100, ctx),
            tx_context::sender(ctx)
        )
    }

    /// Adds more uses to the `Subscription` object.
    public fun topup_pass(s: &mut Subscription<DEVPASS> /* Coin<T> */) {
        dev_pass::add_uses(DEVPASS {}, s, 10)
    }
}

/// Sketch for an application that uses multiple pools.
/// Shows how subscriptions are used in `some_amm` module.
module defi::smart_swapper {
    use defi::some_amm::{Self, DEVPASS};
    use defi::dev_pass::{Self, Subscription};

    // just some types to use as generics for pools
    struct ETH {} struct BTC {} struct KTS {}

    /// Some function that allows cross-pool operations with some additional
    /// logic. The most important part here is the use of subscription to "pay"
    /// for each pool's access.
    entry fun cross_pool_swap(
        s: &mut Subscription<DEVPASS>
        /* assuming a lot of arguments */
    ) {

        let _a = some_amm::dev_swap<ETH, BTC>(dev_pass::use_pass(s) /*, Coin<ETH> */);
        let _b = some_amm::dev_swap<BTC, KTS>(dev_pass::use_pass(s) /*, _a */);

        // do something with swapped values ?
        // transfer::public_transfer( ... )
    }
}
