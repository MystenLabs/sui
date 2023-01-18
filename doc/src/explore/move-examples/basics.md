---
title: Basics
---

This section covers the main features of Sui Move.

## Move.toml

Every Move package has a *package manifest* in the form of a `Move.toml` file - it is placed in the [root of the package](../build/move/index.md#move-code-organization). 

./Move.toml:

```toml
[package]
name = "sui-by-example"
version = "0.1.0"

[dependencies]
Sui = { git = "https://github.com/MystenLabs/sui.git", subdir = "crates/sui-framework", rev = "main" }

[addresses]
examples = "0x0"
```

The manifest itself contains a number of sections, primary of which are:

- `[package]` - includes package metadata such as name and author
- `[dependencies]` - specifies dependencies of the project
- `[addresses]` - address aliases (e.g., `@me` will be treated as a `0x0` address)

Dependencies in the `[dependencies]` section are in the form:

```
<string> = { local = <string>, addr_subst* = { (<string> = (<string> | "<hex_address>"))+ } } # local dependencies
<string> = { git = <URL or path to a git repo>, subdir = <path to dir containing Move.toml inside git repo>, rev = <git commit hash>, addr_subst* = { (<string> = (<string> | "<hex_address>"))+ } } # git dependencies
```

The `addr_subst` option for a dependency enables you to define a placeholder address from another package. It does not, however, provide a means to mutate the address of a published package. For example, a package manifest using a placeholder address might resemble:

```toml
[package]
name = "Child"
version = "0.0.1"

[dependencies]
Sui = { git = "https://github.com/MystenLabs/sui.git", subdir = "crates/sui-framework", rev = "devnet-0.10.0" }

[addresses]
child = "_"
```

The package that references the previous package could then use `addr_subst` to provide the address:

```toml
[package]
name = "Parent"
version = "0.0.1"

[dependencies]
Sui = { git = "https://github.com/MystenLabs/sui.git", subdir = "crates/sui-framework", rev = "devnet-0.10.0" }
Child = { local = "child", addr_subst = { "child" = "0x08f5f5f4101e9c4b2d2b3f212b6e909b48acd02c" } }

[addresses]
parent = "0x0"
```

Using `addr_subst` is recommended when Child is a library that you plan to publish along with the packages that use it.

The `addr_subst` attribute is also useful when multiple child packages use the same unassigned named address. For example, you might have a package A that depends on packages B and C. Both B and C have an unassigned named address, D, but you need to refer to them by unique names in A. In this case, you could supply different named addresses using `addr_subst` and define the addresses in the package A manifest `[addresses]` section.      

Package A Move.toml:

```toml
[package]
name = "A"
version = "0.0.1"

[dependencies]
Sui = { git = "https://github.com/MystenLabs/sui.git", subdir = "crates/sui-framework", rev = "devnet-0.10.0" }
B = { local = "b", addr_subst = { "D" = "bd" }}
C = { local = "c", addr_subst = { "D" = "cd" }}

[addresses]
a = "0x0"
bd = "0x08f5f5f4101e9c4b2d2b3f212b6e909b48acd02b"
cd = "0x04b3d4752496d3663cb274c33e61c732ed44146c"
```

Package B Move.toml:

```toml
[package]
name = "B"
version = "0.0.1"

[dependencies]
Sui = { git = "https://github.com/MystenLabs/sui.git", subdir = "crates/sui-framework", rev = "devnet-0.10.0" }

[addresses]
D = "_"
```

Package C Move.toml:

```toml
[package]
name = "C"
version = "0.0.1"

[dependencies]
Sui = { git = "https://github.com/MystenLabs/sui.git", subdir = "crates/sui-framework", rev = "devnet-0.10.0" }

[addresses]
D = "_"
```



## Init function

Init function is a special function that gets executed only once - when the associated module is published. It always has the same signature and only
one argument:
```move
fun init(ctx: &mut TxContext) { /* ... */ }
```

For example:

```move
module examples::one_timer {
    use sui::transfer;
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};

    /// The one of a kind - created in the module initializer.
    struct CreatorCapability has key {
        id: UID
    }

    /// This function is only called once on module publish.
    /// Use it to make sure something has happened only once, like
    /// here - only module author will own a version of a
    /// `CreatorCapability` struct.
    fun init(ctx: &mut TxContext) {
        transfer::transfer(CreatorCapability {
            id: object::new(ctx),
        }, tx_context::sender(ctx))
    }
}
```


## Entry functions

An [entry function](../build/move/index.md#entry-functions) visibility modifier allows a function to be called directly (e.g., in transaction). It is combinable with other
visibility modifiers, such as `public` which allows calling from other modules) and `public(friend)` for calling from *friend* modules.

```move
module examples::object {
    use sui::transfer;
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;

    struct Object has key {
        id: UID
    }

    /// If function is defined as public - any module can call it.
    /// Non-entry functions are also allowed to have return values.
    public fun create(ctx: &mut TxContext): Object {
        Object { id: object::new(ctx) }
    }

    /// Entrypoints can't have return values as they can only be called
    /// directly in a transaction and the returned value can't be used.
    /// However, `entry` without `public` disallows calling this method from
    /// other Move modules.
    entry fun create_and_transfer(to: address, ctx: &mut TxContext) {
        transfer::transfer(create(ctx), to)
    }
}
```


## Strings

Move does not have a native type for strings, but it has a handy wrapper!

```move
module examples::strings {
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;

    // Use this dependency to get a type wrapper for UTF-8 strings
    use std::string::{Self, String};

    /// A dummy Object that holds a String type
    struct Name has key, store {
        id: UID,

        /// Here it is - the String type
        name: String
    }

    /// Create a name Object by passing raw bytes
    public fun issue_name_nft(
        name_bytes: vector<u8>, ctx: &mut TxContext
    ): Name {
        Name {
            id: object::new(ctx),
            name: string::utf8(name_bytes)
        }
    }
}
```


## Shared object

Shared object is an object that is shared using a `sui::transfer::share_object` function and is accessible to everyone.

```move
/// Unlike `Owned` objects, `Shared` ones can be accessed by anyone on the
/// network. Extended functionality and accessibility of this kind of objects
/// requires additional effort by securing access if needed.
module examples::donuts {
    use sui::transfer;
    use sui::sui::SUI;
    use sui::coin::{Self, Coin};
    use sui::object::{Self, UID};
    use sui::balance::{Self, Balance};
    use sui::tx_context::{Self, TxContext};

    /// For when Coin balance is too low.
    const ENotEnough: u64 = 0;

    /// Capability that grants an owner the right to collect profits.
    struct ShopOwnerCap has key { id: UID }

    /// A purchasable Donut. For simplicity's sake we ignore implementation.
    struct Donut has key { id: UID }

    /// A shared object. `key` ability is required.
    struct DonutShop has key {
        id: UID,
        price: u64,
        balance: Balance<SUI>
    }

    /// Init function is often ideal place for initializing
    /// a shared object as it is called only once.
    ///
    /// To share an object `transfer::share_object` is used.
    fun init(ctx: &mut TxContext) {
        transfer::transfer(ShopOwnerCap {
            id: object::new(ctx)
        }, tx_context::sender(ctx));

        // Share the object to make it accessible to everyone!
        transfer::share_object(DonutShop {
            id: object::new(ctx),
            price: 1000,
            balance: balance::zero()
        })
    }

    /// Entry function available to everyone who owns a Coin.
    public entry fun buy_donut(
        shop: &mut DonutShop, payment: &mut Coin<SUI>, ctx: &mut TxContext
    ) {
        assert!(coin::value(payment) >= shop.price, ENotEnough);

        // Take amount = `shop.price` from Coin<SUI>
        let coin_balance = coin::balance_mut(payment);
        let paid = balance::split(coin_balance, shop.price);

        // Put the coin to the Shop's balance
        balance::join(&mut shop.balance, paid);

        transfer::transfer(Donut {
            id: object::new(ctx)
        }, tx_context::sender(ctx))
    }

    /// Consume donut and get nothing...
    public entry fun eat_donut(d: Donut) {
        let Donut { id } = d;
        object::delete(id);
    }

    /// Take coin from `DonutShop` and transfer it to tx sender.
    /// Requires authorization with `ShopOwnerCap`.
    public entry fun collect_profits(
        _: &ShopOwnerCap, shop: &mut DonutShop, ctx: &mut TxContext
    ) {
        let amount = balance::value(&shop.balance);
        let profits = coin::take(&mut shop.balance, amount, ctx);

        transfer::transfer(profits, tx_context::sender(ctx))
    }
}

```


## Transfer

To make an object freely transferable, use a combination of `key` and `store` abilities.

```move
/// A freely transfererrable Wrapper for custom data.
module examples::wrapper {
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;

    /// An object with `store` can be transferred in any
    /// module without a custom transfer implementation.
    struct Wrapper<T: store> has key, store {
        id: UID,
        contents: T
    }

    /// View function to read contents of a `Container`.
    public fun contents<T: store>(c: &Wrapper<T>): &T {
        &c.contents
    }

    /// Anyone can create a new object
    public fun create<T: store>(
        contents: T, ctx: &mut TxContext
    ): Wrapper<T> {
        Wrapper {
            contents,
            id: object::new(ctx),
        }
    }

    /// Destroy `Wrapper` and get T.
    public fun destroy<T: store> (c: Wrapper<T>): T {
        let Wrapper { id, contents } = c;
        object::delete(id);
        contents
    }
}

module examples::profile {
    use sui::transfer;
    use sui::url::{Self, Url};
    use std::string::{Self, String};
    use sui::tx_context::{Self, TxContext};

    // using Wrapper functionality
    use 0x0::wrapper;

    /// Profile information, not an object, can be wrapped
    /// into a transferable container
    struct ProfileInfo has store {
        name: String,
        url: Url
    }

    /// Read `name` field from `ProfileInfo`.
    public fun name(info: &ProfileInfo): &String {
        &info.name
    }

    /// Read `url` field from `ProfileInfo`.
    public fun url(info: &ProfileInfo): &Url {
        &info.url
    }

    /// Creates new `ProfileInfo` and wraps into `Wrapper`.
    /// Then transfers to sender.
    public fun create_profile(
        name: vector<u8>, url: vector<u8>, ctx: &mut TxContext
    ) {
        // create a new container and wrap ProfileInfo into it
        let container = wrapper::create(ProfileInfo {
            name: string::utf8(name),
            url: url::new_unsafe_from_bytes(url)
        }, ctx);

        // `Wrapper` type is freely transferable
        transfer::transfer(container, tx_context::sender(ctx))
    }
}

```


## Custom transfer

In Sui Move, objects defined with only `key` ability can not be transferred by default. To enable
transfers, publisher has to create a custom transfer function. This function can include any arguments,
for example a fee, that users have to pay to transfer.

```move
module examples::restricted_transfer {
    use sui::tx_context::{Self, TxContext};
    use sui::balance::{Self, Balance};
    use sui::coin::{Self, Coin};
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::sui::SUI;

    /// For when paid amount is not equal to the transfer price.
    const EWrongAmount: u64 = 0;

    /// A Capability that allows bearer to create new `TitleDeed`s.
    struct GovernmentCapability has key { id: UID }

    /// An object that marks a property ownership. Can only be issued
    /// by an authority.
    struct TitleDeed has key {
        id: UID,
        // ... some additional fields
    }

    /// A centralized registry that approves property ownership
    /// transfers and collects fees.
    struct LandRegistry has key {
        id: UID,
        balance: Balance<SUI>,
        fee: u64
    }

    /// Create a `LandRegistry` on module init.
    fun init(ctx: &mut TxContext) {
        transfer::transfer(GovernmentCapability {
            id: object::new(ctx)
        }, tx_context::sender(ctx));

        transfer::share_object(LandRegistry {
            id: object::new(ctx),
            balance: balance::zero<SUI>(),
            fee: 10000
        })
    }

    /// Create `TitleDeed` and transfer it to the property owner.
    /// Only owner of the `GovernmentCapability` can perform this action.
    public entry fun issue_title_deed(
        _: &GovernmentCapability,
        for: address,
        ctx: &mut TxContext
    ) {
        transfer::transfer(TitleDeed {
            id: object::new(ctx)
        }, for)
    }

    /// A custom transfer function. Required due to `TitleDeed` not having
    /// a `store` ability. All transfers of `TitleDeed`s have to go through
    /// this function and pay a fee to the `LandRegistry`.
    public entry fun transfer_ownership(
        registry: &mut LandRegistry,
        paper: TitleDeed,
        fee: Coin<SUI>,
        to: address,
    ) {
        assert!(coin::value(&fee) == registry.fee, EWrongAmount);

        // add a payment to the LandRegistry balance
        balance::join(&mut registry.balance, coin::into_balance(fee));

        // finally call the transfer function
        transfer::transfer(paper, to)
    }
}

```


## Events

Events are the main way to track actions on chain.

```move
/// Extended example of a shared object. Now with addition of events!
module examples::donuts_with_events {
    use sui::transfer;
    use sui::sui::SUI;
    use sui::coin::{Self, Coin};
    use sui::object::{Self, ID, UID};
    use sui::balance::{Self, Balance};
    use sui::tx_context::{Self, TxContext};

    // This is the only dependency you need for events.
    use sui::event;

    /// For when Coin balance is too low.
    const ENotEnough: u64 = 0;

    /// Capability that grants an owner the right to collect profits.
    struct ShopOwnerCap has key { id: UID }

    /// A purchasable Donut. For simplicity's sake we ignore implementation.
    struct Donut has key { id: UID }

    struct DonutShop has key {
        id: UID,
        price: u64,
        balance: Balance<SUI>
    }

    // ====== Events ======

    /// For when someone has purchased a donut.
    struct DonutBought has copy, drop {
        id: ID
    }

    /// For when DonutShop owner has collected profits.
    struct ProfitsCollected has copy, drop {
        amount: u64
    }

    // ====== Functions ======

    fun init(ctx: &mut TxContext) {
        transfer::transfer(ShopOwnerCap {
            id: object::new(ctx)
        }, tx_context::sender(ctx));

        transfer::share_object(DonutShop {
            id: object::new(ctx),
            price: 1000,
            balance: balance::zero()
        })
    }

    /// Buy a donut.
    public entry fun buy_donut(
        shop: &mut DonutShop, payment: &mut Coin<SUI>, ctx: &mut TxContext
    ) {
        assert!(coin::value(payment) >= shop.price, ENotEnough);

        let coin_balance = coin::balance_mut(payment);
        let paid = balance::split(coin_balance, shop.price);
        let id = object::new(ctx);

        balance::join(&mut shop.balance, paid);

        // Emit the event using future object's ID.
        event::emit(DonutBought { id: object::uid_to_inner(&id) });
        transfer::transfer(Donut { id }, tx_context::sender(ctx))
    }

    /// Consume donut and get nothing...
    public entry fun eat_donut(d: Donut) {
        let Donut { id } = d;
        object::delete(id);
    }

    /// Take coin from `DonutShop` and transfer it to tx sender.
    /// Requires authorization with `ShopOwnerCap`.
    public entry fun collect_profits(
        _: &ShopOwnerCap, shop: &mut DonutShop, ctx: &mut TxContext
    ) {
        let amount = balance::value(&shop.balance);
        let profits = coin::take(&mut shop.balance, amount, ctx);

        // simply create new type instance and emit it
        event::emit(ProfitsCollected { amount });

        transfer::transfer(profits, tx_context::sender(ctx))
    }
}

```


## One time witness

One Time Witness (OTW) is a special instance of a type which is created only in the module initializer and is guaranteed to be unique and have only one instance. It is important for cases where we need to make sure that a witness-authorized action was performed only once (for example - [creating a new Coin](../explore/move-examples/samples.md#coin)). In Sui Move a type is considered an OTW if its definition has the following properties:

- Named after the module but uppercased
- Has only `drop` ability

> To check whether an instance is an OTW, [`sui::types::is_one_time_witness(witness)`](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/types.move) should be used.

To get an instance of this type, you need to add it as the first argument to the `init()` function: Sui runtime supplies both initializer arguments automatically.

```move
module examples::mycoin {

    /// Name matches the module name
    struct MYCOIN has drop {}

    /// The instance is received as the first argument
    fun init(witness: MYCOIN, ctx: &mut TxContext) {
        /* ... */
    }
}
```

---

Example which illustrates how OTW could be used:

```move
/// This example illustrates how One Time Witness works.
///
/// One Time Witness (OTW) is an instance of a type which is guaranteed to
/// be unique across the system. It has the following properties:
///
/// - created only in module initializer
/// - named after the module (uppercased)
/// - cannot be packed manually
/// - has a `drop` ability
module examples::one_time_witness_registry {
    use sui::tx_context::TxContext;
    use sui::object::{Self, UID};
    use std::string::String;
    use sui::transfer;

    // This dependency allows us to check whether type
    // is a one-time witness (OTW)
    use sui::types;

    /// For when someone tries to send a non OTW struct
    const ENotOneTimeWitness: u64 = 0;

    /// An object of this type will mark that there's a type,
    /// and there can be only one record per type.
    struct UniqueTypeRecord<phantom T> has key {
        id: UID,
        name: String
    }

    /// Expose a public function to allow registering new types with
    /// custom names. With a `is_one_time_witness` call we make sure
    /// that for a single `T` this function can be called only once.
    public fun add_record<T: drop>(
        witness: T,
        name: String,
        ctx: &mut TxContext
    ) {
        // This call allows us to check whether type is an OTW;
        assert!(types::is_one_time_witness(&witness), ENotOneTimeWitness);

        // Share the record for the world to see. :)
        transfer::share_object(UniqueTypeRecord<T> {
            id: object::new(ctx),
            name
        });
    }
}

/// Example of spawning an OTW.
module examples::my_otw {
    use std::string;
    use sui::tx_context::TxContext;
    use examples::one_time_witness_registry as registry;

    /// Type is named after the module but uppercased
    struct MY_OTW has drop {}

    /// To get it, use the first argument of the module initializer.
    /// It is a full instance and not a reference type.
    fun init(witness: MY_OTW, ctx: &mut TxContext) {
        registry::add_record(
            witness, // here it goes
            string::utf8(b"My awesome record"),
            ctx
        )
    }
}

```