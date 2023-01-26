---
title: Patterns
---

This part covers the programming patterns that are widely used in Move; some of which can exist only in Move.


## Capability

Capability is a pattern that allows *authorizing* actions with an object. One of the most common capabilities is `TreasuryCap` (defined in [sui::coin](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/coin.move#L19)).


```move
module examples::item {
    use sui::transfer;
    use sui::object::{Self, UID};
    use std::string::{Self, String};
    use sui::tx_context::{Self, TxContext};

    /// Type that marks Capability to create new `Item`s.
    struct AdminCap has key { id: UID }

    /// Custom NFT-like type.
    struct Item has key, store { id: UID, name: String }

    /// Module initializer is called once on module publish.
    /// Here we create only one instance of `AdminCap` and send it to the publisher.
    fun init(ctx: &mut TxContext) {
        transfer::transfer(AdminCap {
            id: object::new(ctx)
        }, tx_context::sender(ctx))
    }

    /// The entry function can not be called if `AdminCap` is not passed as
    /// the first argument. Hence only owner of the `AdminCap` can perform
    /// this action.
    public entry fun create_and_send(
        _: &AdminCap, name: vector<u8>, to: address, ctx: &mut TxContext
    ) {
        transfer::transfer(Item {
            id: object::new(ctx),
            name: string::utf8(name)
        }, to)
    }
}

```

## Witness

Witness is a pattern that is used for confirming the ownership of a type. To do so, pass a `drop` instance of a type. Coin relies on this implementation.

```move
/// Module that defines a generic type `Guardian<T>` which can only be
/// instantiated with a witness.
module examples::guardian {
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;

    /// Phantom parameter T can only be initialized in the `create_guardian`
    /// function. But the types passed here must have `drop`.
    struct Guardian<phantom T: drop> has key, store {
        id: UID
    }

    /// The first argument of this function is an actual instance of the
    /// type T with `drop` ability. It is dropped as soon as received.
    public fun create_guardian<T: drop>(
        _witness: T, ctx: &mut TxContext
    ): Guardian<T> {
        Guardian { id: object::new(ctx) }
    }
}

/// Custom module that makes use of the `guardian`.
module examples::peace_guardian {
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    // Use the `guardian` as a dependency.
    use 0x0::guardian;

    /// This type is intended to be used only once.
    struct PEACE has drop {}

    /// Module initializer is the best way to ensure that the
    /// code is called only once. With `Witness` pattern it is
    /// often the best practice.
    fun init(ctx: &mut TxContext) {
        transfer::transfer(
            guardian::create_guardian(PEACE {}, ctx),
            tx_context::sender(ctx)
        )
    }
}

```

This pattern is used in these examples:

- [Liquidity pool](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/defi/sources/pool.move)
- [Regulated coin](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/fungible_tokens/sources/regulated_coin.move)


## Transferable witness

```move
/// This pattern is based on a combination of two others: Capability and a Witness.
/// Since Witness is something to be careful with, spawning it should be allowed
/// only to authorized users (ideally only once). But some scenarios require
/// type authorization by module X to be used in another module Y. Or, possibly,
/// there's a case where authorization should be performed after some time.
///
/// For these rather rare scerarios, a storable witness is a perfect solution.
module examples::transferable_witness {
    use sui::transfer;
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};

    /// Witness now has a `store` that allows us to store it inside a wrapper.
    struct WITNESS has store, drop {}

    /// Carries the witness type. Can be used only once to get a Witness.
    struct WitnessCarrier has key { id: UID, witness: WITNESS }

    /// Send a `WitnessCarrier` to the module publisher.
    fun init(ctx: &mut TxContext) {
        transfer::transfer(
            WitnessCarrier { id: object::new(ctx), witness: WITNESS {} },
            tx_context::sender(ctx)
        )
    }

    /// Unwrap a carrier and get the inner WITNESS type.
    public fun get_witness(carrier: WitnessCarrier): WITNESS {
        let WitnessCarrier { id, witness } = carrier;
        object::delete(id);
        witness
    }
}

```


## Hot potato

Hot Potato is a name for a struct that has no abilities, hence it can only be packed and unpacked in its module. In this struct, you must call function B after function A in the case where function A returns a potato and function B consumes it.

```move
module examples::trade_in {
    use sui::transfer;
    use sui::sui::SUI;
    use sui::coin::{Self, Coin};
    use sui::object::{Self, UID};
    use sui::tx_context::{TxContext};

    /// Price for the first phone model in series
    const MODEL_ONE_PRICE: u64 = 10000;

    /// Price for the second phone model
    const MODEL_TWO_PRICE: u64 = 20000;

    /// For when someone tries to purchase non-existing model
    const EWrongModel: u64 = 1;

    /// For when paid amount does not match the price
    const EIncorrectAmount: u64 = 2;

    /// A phone; can be purchased or traded in for a newer model
    struct Phone has key, store { id: UID, model: u8 }

    /// Payable receipt. Has to be paid directly or paid with a trade-in option.
    /// Cannot be stored, owned or dropped - has to be used to select one of the
    /// options for payment: `trade_in` or `pay_full`.
    struct Receipt { price: u64 }

    /// Get a phone, pay later.
    /// Receipt has to be passed into one of the functions that accept it:
    ///  in this case it's `pay_full` or `trade_in`.
    public fun buy_phone(model: u8, ctx: &mut TxContext): (Phone, Receipt) {
        assert!(model == 1 || model == 2, EWrongModel);

        let price = if (model == 1) MODEL_ONE_PRICE else MODEL_TWO_PRICE;

        (
            Phone { id: object::new(ctx), model },
            Receipt { price }
        )
    }

    /// Pay the full price for the phone and consume the `Receipt`.
    public fun pay_full(receipt: Receipt, payment: Coin<SUI>) {
        let Receipt { price } = receipt;
        assert!(coin::value(&payment) == price, EIncorrectAmount);

        // for simplicity's sake transfer directly to @examples account
        transfer::transfer(payment, @examples);
    }

    /// Give back an old phone and get 50% of its price as a discount for the new one.
    public fun trade_in(receipt: Receipt, old_phone: Phone, payment: Coin<SUI>) {
        let Receipt { price } = receipt;
        let tradein_price = if (old_phone.model == 1) MODEL_ONE_PRICE else MODEL_TWO_PRICE;
        let to_pay = price - (tradein_price / 2);

        assert!(coin::value(&payment) == to_pay, EIncorrectAmount);

        transfer::transfer(old_phone, @examples);
        transfer::transfer(payment, @examples);
    }
}

```

This pattern is used in these examples:

- [Flash Loan](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/defi/sources/flash_lender.move)


## ID pointer

ID Pointer is a technique that separates the main data (an object) and its accessors / capabilities by linking the latter to the original. There's a few different directions in which you can use this pattern:

- issuing transferable capabilities for shared objects (for example, a TransferCap that changes 'owner' field of a shared object)
- splitting dynamic data and static (for example, an NFT and its Collection information)
- avoiding unnecessary type linking (and witness requirement) in generic applications (LP token for a LiquidityPool)

```move
/// This example implements a simple `Lock` and `Key` mechanics
/// on Sui where `Lock<T>` is a shared object that can contain any object,
/// and `Key` is an owned object which is required to get access to the
/// contents of the lock.
///
/// `Key` is linked to its `Lock` using an `ID` field. This check allows
/// off-chain discovery of the target as well as splits the dynamic
/// transferable capability and the 'static' contents. Another benefit of
/// this approach is that the target asset is always discoverable while its
/// `Key` can be wrapped into another object (eg a marketplace listing).
module examples::lock_and_key {
    use sui::object::{Self, ID, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use std::option::{Self, Option};

    /// Lock is empty, nothing to take.
    const ELockIsEmpty: u64 = 0;

    /// Key does not match the Lock.
    const EKeyMismatch: u64 = 1;

    /// Lock already contains something.
    const ELockIsFull: u64 = 2;

    /// Lock that stores any content inside it.
    struct Lock<T: store + key> has key {
        id: UID,
        locked: Option<T>
    }

    /// A key that is created with a Lock; is transferable
    /// and contains all the needed information to open the Lock.
    struct Key<phantom T: store + key> has key, store {
        id: UID,
        for: ID,
    }

    /// Returns an ID of a Lock for a given Key.
    public fun key_for<T: store + key>(key: &Key<T>): ID {
        key.for
    }

    /// Lock some content inside a shared object. A Key is created and is
    /// sent to the transaction sender. For example, we could turn the
    /// lock into a treasure chest by locking some `Coin<SUI>` inside.
    ///
    /// Sender gets the `Key` to this `Lock`.
    public entry fun create<T: store + key>(obj: T, ctx: &mut TxContext) {
        let id = object::new(ctx);
        let for = object::uid_to_inner(&id);

        transfer::share_object(Lock<T> {
            id,
            locked: option::some(obj),
        });

        transfer::transfer(Key<T> {
            for,
            id: object::new(ctx)
        }, tx_context::sender(ctx));
    }

    /// Lock something inside a shared object using a Key. Aborts if
    /// lock is not empty or if key doesn't match the lock.
    public entry fun lock<T: store + key>(
        obj: T,
        lock: &mut Lock<T>,
        key: &Key<T>,
    ) {
        assert!(option::is_none(&lock.locked), ELockIsFull);
        assert!(&key.for == object::borrow_id(lock), EKeyMismatch);

        option::fill(&mut lock.locked, obj);
    }

    /// Unlock the Lock with a Key and access its contents.
    /// Can only be called if both conditions are met:
    /// - key matches the lock
    /// - lock is not empty
    public fun unlock<T: store + key>(
        lock: &mut Lock<T>,
        key: &Key<T>,
    ): T {
        assert!(option::is_some(&lock.locked), ELockIsEmpty);
        assert!(&key.for == object::borrow_id(lock), EKeyMismatch);

        option::extract(&mut lock.locked)
    }

    /// Unlock the Lock and transfer its contents to the transaction sender.
    public fun take<T: store + key>(
        lock: &mut Lock<T>,
        key: &Key<T>,
        ctx: &mut TxContext,
    ) {
        transfer::transfer(unlock(lock, key), tx_context::sender(ctx))
    }
}

```

This pattern is used in these examples:

- [Lock](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/basics/sources/lock.move)
- [Escrow](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/defi/sources/escrow.move)
- [Hero](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/games/sources/hero.move)
- [Tic Tac Toe](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/games/sources/tic_tac_toe.move)
- [Auction](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/nfts/sources/auction.move)