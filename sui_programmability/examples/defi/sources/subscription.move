/// Module implementing generic Subscription with limited number of uses.
///
/// This specific example shows how this model can be used within the AMM
/// context to charge developers who make use of multiple shared objects
/// which is expected to slow down all pools used.
///
/// The module itself makes use of HotPotato and Witness-based interface
/// which can be implemented by other applications based on their needs.
module defi::dev_pass {
    use sui::tx_context::{TxContext};
    use sui::object::{Self, Info};
    use sui::transfer;

    /// Owned object from which SingleUses are spawn
    struct Subscription<phantom T> has key {
        info: Info,
        uses: u64
    }

    /// A single use potato.
    struct SingleUse<phantom T> {}

    /// Function to issue a membership with a number of uses.
    public fun issue_subscription<T: drop>(_w: T, uses: u64, ctx: &mut TxContext): Subscription<T> {
        Subscription {
            info: object::new(ctx),
            uses
        }
    }

    /// Confirm a use of a pass. Verified by the Mambership-supported App through Witness.
    public fun confirm_use<T: drop>(_w: T, pass: SingleUse<T>) {
        let SingleUse { } = pass;
    }

    /// When subscription is owned, create HotPotato to use in the service.
    public fun use_pass<T>(membership: &mut Subscription<T>): SingleUse<T> {
        assert!(membership.uses != 0, 0);
        membership.uses = membership.uses - 1;
        SingleUse {}
    }

    /// Allow applications customize transferability of the subscription.
    public fun transfer<T: drop>(_w: T, s: Subscription<T>, to: address) {
        transfer::transfer(s, to)
    }
}

module defi::some_amm {
    use defi::dev_pass::{Self, SingleUse};
    use sui::tx_context::{Self, TxContext};

    /// A type to Mark subscription
    struct DEVPASS has drop {}
    /* Can be customized to: DEVPASS<phantom T, phantom S> to make one subscription per pool */
    /* And the price could be determined based on amount LP tokens issued or trade volume */

    /// Entry function that uses a shared pool object.
    /// Can only be accessed from outside of the chain - can't be "tooled"
    entry fun swap<T, S>(/* &mut Pool, Coin<T> ... */) { /* ... */ }

    /// Function similar to the `swap` but can be called from other Move modules.
    /// Opens up tooling; potentially slows down the AMM if multiple shared objects
    /// used externally! And for that developers have to pay.
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
}

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

        let _a = some_amm::dev_swap<ETH, BTC>(dev_pass::use_pass(s));
        let _b = some_amm::dev_swap<BTC, KTS>(dev_pass::use_pass(s));

        // do something about the swapped values ?
        // transfer::transfer( ... )
    }
}
