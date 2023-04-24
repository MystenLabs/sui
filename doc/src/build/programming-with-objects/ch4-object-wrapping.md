---
title: Chapter 4 - Object Wrapping
---

In many programming languages, you organize data structures in layers by nesting complex data structures in another data structure. In Sui Move, you can organize data structures by putting a field of `struct` type in another, like the following:

```rust
struct Foo has key {
    id: UID,
    bar: Bar,
}

struct Bar has store {
    value: u64,
}
```

To embed a struct type in a Sui object struct (with a `key` ability), the struct type must have the `store` ability.

In the preceding example, `Bar` is a normal struct, but it is not a Sui object since it doesn't have the `key` ability. This is common usage to organize data with good encapsulation.

To put a Sui object struct type as a field in another Sui object struct type, change `Bar` into:

```rust
struct Bar has key, store {
    id: UID,
    value: u64,
}
```

Now `Bar` is also a Sui object type. If you put a Sui object of type `Bar` into a Sui object of type `Foo`, the object type `Foo` wraps the object type `Bar`. The object type `Foo` is the wrapper or wrapping object.

In Sui Move code, you can put a Sui object as a field of a non-Sui object struct type. For example, the preceding code sample defined `Foo` to not have `key`, but `Bar` to have `key, store`. This case can happen only temporarily in the middle of a Sui Move execution, and cannot persist on-chain. This is because a non-Sui object cannot flow across the Move-Sui boundary, and one must unpack the non-Sui object at some point and handle the Sui object fields in it.

There are some interesting consequences of wrapping a Sui object into another. When an object is wrapped, the object no longer exists independently on-chain. You can no longer look up the object by its ID. The object becomes part of the data of the object that wraps it. Most importantly, you can no longer pass the wrapped object as an argument in any way in Sui Move calls. The only access point is through the wrapping object.

It is not possible to create circular wrapping behavior, where A wraps B, B wraps C, and C also wraps A.

At some point, you can then take out the wrapped object and transfer it to an address. This is called **unwrapping**. When an object is **unwrapped**, it becomes an independent object again, and can be accessed directly on-chain. There is also an important property about wrapping and unwrapping: the object's ID stays the same across wrapping and unwrapping.

There are a few common ways to wrap a Sui object into another Sui object, and their use cases are typically different. This section describes three different ways to wrap a Sui object with typical use cases.

### Direct wrapping

If you put a Sui object type directly as a field in another Sui object type (as in the preceding example), it is called _direct wrapping_. The most important property achieved through direct wrapping is that the wrapped object cannot be unwrapped unless the wrapping object is destroyed. In the preceding example, to make `Bar` a standalone object again, delete (and hence [unpack](https://move-book.com/advanced-topics/struct.html#destructing-structures)) the `Foo` object. Direct wrapping is the best way to implement object locking, which is to lock an object with constrained access. You can unlock it only through specific contract calls.

The following example implementation of a trusted swap demonstrates how to use direct wrapping. Assume there is an NFT-style `Object` type that has `scarcity` and `style`. In this example, `scarcity` determines how rare the object is (presumably the more scarce the higher its market value), and `style` determines the object content/type or how it's rendered. If you own some of these objects and want to trade your objects with others, you want to make sure it's a fair trade. You are willing to trade an object only with another one that has identical `scarcity`, but want a different `style` (so that you can collect more styles).

First, define such an object type:

```rust
struct Object has key, store {
    id: UID,
    scarcity: u8,
    style: u8,
}
```

In a real application, you might make sure that there is a limited supply of the objects, and there is a mechanism to mint them to a list of owners. For simplicity and demonstration purposes, this example simplifies creation:

```rust
public entry fun create_object(scarcity: u8, style: u8, ctx: &mut TxContext) {
    let object = Object {
        id: object::new(ctx),
        scarcity,
        style,
    };
    transfer::transfer(object, tx_context::sender(ctx))
}
```

Anyone can call `create_object` to create a new object with specified `scarcity` and `style`. The created object is sent to the signer of the transaction. To enable transferring the object to others:

```rust
public entry fun transfer_object(object: Object, recipient: address) {
    transfer::transfer(object, recipient)
}
```

You can also enable a swap/trade between your object and others' objects. For example, define a function that takes two objects from two addresses and swaps their ownership. But this doesn't work in Sui! Recall from [Using Objects](ch2-using-objects.md) that only object owners can send a transaction to mutate the object. So one person cannot send a transaction that would swap their own object with someone else's object.

Sui supports multi-signature (multi-sig) transactions so that two people can sign the same transaction for this type of use case. But a multi-sig transaction doesn't work in this scenario. 

Another common solution is to send your object to a pool - such as an NFT marketplace or a staking pool - and perform the swap in the pool (either right away, or later when there is demand). Other chapters explore the concept of shared objects that can be mutated by anyone, and show that how it enables anyone to operate in a shared object pool. This chapter focuses on how to achieve the same effect using owned objects. Transactions using only owned objects are faster and less expensive (in terms of gas) than using shared objects, since they do not require consensus in Sui.

To swap objects, the same address must own both objects. Anyone who wants to swap their object can send their objects to the third party, such as a site that offers swapping services, and the third party helps perform the swap and send the objects to the appropriate owner. To ensure that you retain custody of your objects (such as coins and NFTs) and not give full custody to the third party, use direct wrapping. To define a wrapper object type:

```rust
struct ObjectWrapper has key {
    id: UID,
    original_owner: address,
    to_swap: Object,
    fee: Balance<SUI>,
}
```

`ObjectWrapper` defines a Sui object type, wraps the object to swap as `to_swap`, and tracks the original owner of the object in `original_owner`. You might need to also pay the third party some fee for this swap. To define an interface to request a swap by someone who owns an `Object`:

```rust
public entry fun request_swap(object: Object, fee: Coin<SUI>, service_address: address, ctx: &mut TxContext) {
    assert!(coin::value(&fee) >= MIN_FEE, 0);
    let wrapper = ObjectWrapper {
        id: object::new(ctx),
        original_owner: tx_context::sender(ctx),
        to_swap: object,
        fee: coin::into_balance(fee),
    };
    transfer::transfer(wrapper, service_address);
}
```

In the preceding entry function, you must pass the object by value so that it's fully consumed and wrapped into `ObjectWrapper`to request swapping an `object`. The example also provides a fee (in the type of `Coin<SUI>`). The function also checks that the fee is sufficient. The example turns `Coin` into `Balance` when it's put into the `wrapper` object. This is because `Coin` is a Sui object type and used only to pass around as Sui objects (such as entry function arguments or objects sent to addresses). For coin balances that need to be embedded in another Sui object struct, use `Balance` instead because it's not a Sui object type and is much less expensive to use.

The wrapper object is then sent to the service operator, with the address specified in the call as `service_address`.

Although the service operator (`service_address`) now owns the `ObjectWrapper`, which contains the object to be swapped, the service operator still cannot access or steal the underlying wrapped `Object`. This is because the `transfer_object` function you defined requires you to pass an `Object` into it; but the service operator cannot access the wrapped `Object`, and passing `ObjectWrapper` to the `transfer_object` function would be invalid. Recall that an object can be read or modified only by the module in which it is defined; because this module defines only a wrapping / packing function (`request_swap`), and not an unwrapping / unpacking function, the service operator has no way to unpack the `ObjectWrapper` to retrieve the wrapped `Object`. Also, `ObjectWrapper` itself lacks any defined transfer method, so the service operator cannot transfer the wrapped object to someone else either.

The function interface for the function that the service operator can call to perform a swap between two objects sent from two addresses resembles:

```rust
public entry fun execute_swap(wrapper1: ObjectWrapper, wrapper2: ObjectWrapper, ctx: &mut TxContext);
```

Where `wrapper1` and `wrapper2` are two wrapped objects that were sent from different object owners to the service operator. Both wrapped objects are passed by value because they eventually need to be [unpacked](https://move-book.com/advanced-topics/struct.html#destructing-structures).

First, check that the swap is legitimate:

```rust
assert!(wrapper1.to_swap.scarcity == wrapper2.to_swap.scarcity, 0);
assert!(wrapper1.to_swap.style != wrapper2.to_swap.style, 0);
```

It checks that the two objects have identical scarcity, but have different styles. This meets your criteria for a swap. Unpacking the two objects to obtain the inner fields, also unwraps the objects:

```rust
let ObjectWrapper {
    id: id1,
    original_owner: original_owner1,
    to_swap: object1,
    fee: fee1,
} = wrapper1;

let ObjectWrapper {
    id: id2,
    original_owner: original_owner2,
    to_swap: object2,
    fee: fee2,
} = wrapper2;
```

To perform the actual swap:

```rust
transfer::transfer(object1, original_owner2);
transfer::transfer(object2, original_owner1);
```

The preceding code sends `object1` to the original owner of `object2`, and sends `object2` to the original owner of `object1`. The service provider also takes a fee:

```rust
let service_address = tx_context::sender(ctx);
balance::join(&mut fee1, fee2);
transfer::transfer(coin::from_balance(fee1, ctx), service_address);
```

The `fee2` is merged into `fee1`, turned into a `Coin` and sent to the `service_address`. Finally, delete  both wrapped objects: 

```rust
object::delete(id1);
object::delete(id2);
```

After this call, the two objects are swapped and the service provider takes the service fee.

Since the contract defined only one way to deal with `ObjectWrapper` - `execute_swap` - there is no other way the service operator can interact with `ObjectWrapper` despite its ownership.

Find the full source code in [trusted_swap.move](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/objects_tutorial/sources/trusted_swap.move).

To view a more complex example of how to use direct wrapping, see [escrow.move](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/defi/sources/escrow.move).

### Wrapping through `Option`

When Sui object type `Bar` is directly wrapped into `Foo`, there is not much flexibility: a `Foo` object must have a `Bar` object in it, and to take out the `Bar` object you must destroy the `Foo` object. However, for more flexibility, the wrapping type might not always have the wrapped object in it, and the wrapped object might be replaced with a different object at some point.

To demonstrate this use case, design a simple game character: A warrior with a sword and shield. A warrior might have a sword and shield, or might not have either. The warrior should be able to add a sword and shield, and replace the current ones at any time. To design this, define a `SimpleWarrior` type:

```rust
struct SimpleWarrior has key {
    id: UID,
    sword: Option<Sword>,
    shield: Option<Shield>,
}
```

Each `SimpleWarrior` type has an optional `sword` and `shield` wrapped in it, defined as:

```rust
struct Sword has key, store {
    id: UID,
    strength: u8,
}

struct Shield has key, store {
    id: UID,
    armor: u8,
}
```

When you create a new warrior, set the `sword` and `shield` to `none` to indicate there is no equipment yet:

```rust
public entry fun create_warrior(ctx: &mut TxContext) {
    let warrior = SimpleWarrior {
        id: object::new(ctx),
        sword: option::none(),
        shield: option::none(),
    };
    transfer::transfer(warrior, tx_context::sender(ctx))
}
```

You can then define functions to equip new swords or new shields:

```rust
public entry fun equip_sword(warrior: &mut SimpleWarrior, sword: Sword, ctx: &mut TxContext) {
    if (option::is_some(&warrior.sword)) {
        let old_sword = option::extract(&mut warrior.sword);
        transfer::transfer(old_sword, tx_context::sender(ctx));
    };
    option::fill(&mut warrior.sword, sword);
}
```

The function in the preceding example passes a `warrior` as a mutable reference of `SimpleWarrior`, and passes a `sword` by value to wrap it into the `warrior`.

Note that because `Sword` is a Sui object type without `drop` ability, if the warrior already has a sword equipped, the warrior can't drop that sword. If you call `option::fill` without first checking and taking out the existing sword, an error occurs. In `equip_sword`, first check whether there is already a sword equipped. If so, remove it out and send it back to the sender. To a player, this returns an equipped sword to their inventory when they equip the new sword.

Find the source code in [simple_warrior.move](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/objects_tutorial/sources/simple_warrior.move).

To view a more complex example, see [hero.move](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/games/sources/hero.move).

### Wrapping through `vector`

The concept of wrapping objects in a vector field of another Sui object is very similar to wrapping through `Option`: an object can contain 0, 1, or many of the wrapped objects of the same type.

Wrapping through vector resembles:

```rust
struct Pet has key, store {
    id: UID,
    cuteness: u64,
}

struct Farm has key {
    id: UID,
    pets: vector<Pet>,
}
```

The preceding example wraps a vector of `Pet` in `Farm`, and can be accessed only through the `Farm` object.