/// Unlike `Owned` objects, `Shared` ones can be accessed by anyone on the
/// network. Extended functionality and accessibility of this kind of objects
/// requires additional effort by securing access if needed.
module 0x0::shared {
    use sui::transfer;
    use sui::sui::SUI;
    use sui::id::{VersionedID};
    use sui::coin::{Self, Coin};
    use sui::balance::{Self, Balance};
    use sui::tx_context::{Self, TxContext};

    /// For when Coin balance is too low.
    const ENotEnough: u64 = 0;

    /// A purchased item. For simplicity's sake we ignore implementation.
    struct Item has key { id: VersionedID }

    /// A shared object. `key` ability is required.
    struct Shop has key {
        id: VersionedID,
        price: u64,
        balance: Balance<SUI>
    }

    /// Init function is often ideal place for initializing
    /// a shared object as it is called only once.
    ///
    /// To share an object `transfer::share_object` is used.
    fun init(ctx: &mut TxContext) {
        transfer::share_object(Shop {
            id: tx_context::new_id(ctx),
            price: 1000,
            balance: balance::zero()
        })
    }

    /// Entry function available to everyone who owns a Coin.
    public entry fun buy_item(
        shop: &mut Shop, payment: &mut Coin<SUI>, ctx: &mut TxContext
    ) {
        assert!(coin::value(payment) >= shop.price, ENotEnough);

        // Take amount = `shop.price` from Coin<SUI>
        let coin_balance = coin::balance_mut(payment);
        let paid = balance::split(coin_balance, shop.price);

        // Put the coin to the Shop's balance
        balance::join(&mut shop.balance, paid);

        transfer::transfer(Item {
            id: tx_context::new_id(ctx)
        }, tx_context::sender(ctx))
    }
}
