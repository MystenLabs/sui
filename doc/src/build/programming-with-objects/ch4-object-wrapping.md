## Chapter 4: Object Wrapping
In most languages, we organize data structures in layer by nesting complex data structures in another data structure. In Move, you could do the same by putting a field of struct type in another, like the following:
```rust
struct Foo has key {
    id: VersionedID,
    bar: Bar,
}
struct Bar has store {
    value: u64,
}
```
> :bulb: For a struct type to be capable of being embedded in a Sui object struct (which will have `key` ability), the embedded struct type must have `store` ability.

In the above example, `Bar` is a normal Move struct that cannot be used to construct Sui objects, since it doesn't have `key` ability. This is very common usage when we just need to organize data with good encapsulation.
In some cases, however, we want to put a Sui object struct type as a field in another Sui object struct type. In the above example, if we change `Bar` into:
```rust
struct Bar has key, store {
    id: VersionedID,
    value: u64,
}
```
Now `Bar` is also a Sui object type. When we put a Sui object of type `Bar` into a Sui object of type `Foo`, the Sui object of type `Bar` is said to be **wrapped**.
There are some interesting consequences of wrapping an Sui object into another. When an object is wrapped, this object no longer exists independently on-chain. One will no longer be able to look up this object by its ID. This object becomes part of the data of the object that wraps it. Most importantly, one can no longer pass the wrapped object as an argument in any way in Move calls. The only access point is through the wrapping object.
There are a few common ways to wrap a Sui object into another Sui object, and there use cases are likely different. In the following, we will walk through three different ways to wrap a Sui object and their typical use cases.

### Direct Wrapping
When we put a Sui object type directly as a field in another Sui object type (just like how we put `Bar` as field `bar` in `Foo`), it is called direct wrapping. The most important property achieved through direct wrapping is the following: The wrapped object cannot be unwrapped unless we destroy the wrapping object. In the examle above, in order to make `bar` a standalone object again, one has to delete (and hence [unpack](https://move-book.com/advanced-topics/struct.html#destructing-structures)) the `Foo` object. Because of this property, Direct Wrapping is the best way to implement **Object Locking**: lock an object with constrained access, and one can only unlock it through specific contract calls.

Let's walk through an example implementation of a trusted swap to demonstrate how to use direct wrapping. Let's say there is an NFT-style `Object` type that has `scarcity` and `style`. `scarcity` determines how scarce the object is (presumably the more scarce the higher its market value); `style` determines the object content/type or how it's rendered. You own some of these objects, and want to trade your objects with others. But to make sure it's a fair trade, you are only willing to trade an object with another one that has identical `scarcity` but different `style` (so that you can collect more styles).

First of all, let's define such an object type:
```rust
struct Object has key, store {
    id: VersionedID,
    scarcity: u8,
    style: u8,
}
```
In a real application, we probably would make sure that there is a limited supply of the objects and there is a mechanism to mint them to a list of owners. For simplicity and demonstration purpose, here we will just make it straightforward to create:
```rust
public(script) fun create_object(scarcity: u8, style: u8, ctx: &mut TxContext) {
    let object = Object {
        id: TxContext::new_id(ctx),
        scarcity,
        style,
    };
    Transfer::transfer(object, TxContext::sender(ctx))
}
```
Anyone can call `create_object` to create a new object with specified `scarcity` and `style`. The created object will be sent to the signer of the transaction. We will likely also want to be able to transfer the object to others:
```rust
public(script) fun transfer_object(object: Object, ctx: &mut TxContext) {
    Transfer::transfer(object, TxContext::sender(ctx))
}
```

Now let's look at how we could enable a swap/trade between your object and others' objects. A straightforward idea is this: define a function that takes two objects from two accounts, and swap their ownership. But this doesn't work in Sui! Recall from Chapter 2 that only object owners can send a transaction to mutate the object. So one person cannot send a transaction that would swap their own object with someone else's object.

In the future, we will likely introduce multi-sig transactions so that two people can sign the same transaction for this type of usecase. However you may not always be able to find someone to swap with right away. A multi-sig transaction won't work in this scenario. Even if you can, you may not want to carry the burden of finding a swap target.

Another common solution is to "send" your object to a pool (e.g. a marketplace in the case of NFT, or a liquidity pool in the case of tokens), and perform the swap in the pool (either right away, or latter when there is demand). In future chapters, we will explore the concept of shared objects that can be mutated by anyone, and show that how it enables anyone to operate in a shared object pool. In this chapter, we will focus on how to achieve the same effect using owned objects. Transactions only using owned objects is significantly faster and cheapter (in terms of gas) than using shared objects, since it does not require consensus in Sui.

To be able to perform a swap of objects, both objects must be owned by the same account. We can imagine that a third-party builds infrastructure to provide swap serviecs. Anyone who wants to swap their object can send their objects to the third-party, and the third-party will help perform the swap and send them back. However, we don't fully trust the third-party and don't want to give them full custody of our objects. To achieve this, we can use direct wrapping. We define a wrapper object type as following:
```rust
struct ObjectWrapper has key {
    id: VersionedID,
    original_owner: address,
    to_swap: Object,
    fee: Balance<SUI>,
}
```
`ObjectWrapper` defines a Sui object type, wraps the object that we want to swap as `to_swap`, and tracks the original owner of the object in `original_owner`. To make this more interesting and realistic, we can also expect that we may need to pay the third-party some fee for this swap. Below we define an interface to request a swap by someone who owns an `Object`:
```rust
public(script) fun request_swap(object: Object, fee: Coin<SUI>, service_address: address, ctx: &mut TxContext) {
    assert!(Coin::value(&fee) >= MIN_FEE, 0);
    let wrapper = ObjectWrapper {
        id: TxContext::new_id(ctx),
        original_owner: TxContext::sender(ctx),
        to_swap: object,
        fee: Coin::into_balance(fee),
    };
    Transfer::transfer(wrapper, service_address);
}
```
In the above entry function, to request for swapping an `object`, one must pass the object by value so that it's fully consumed and wrapped into `ObjectWrapper`. A fee (in the type of `Coin<SUI>`) is also provided. It also checks that the fee is sufficient. Note that we turn `Coin` into `Balance` when putting it into the `wrapper` object. This is because `Coin` is a Sui object type and only used to pass around as Sui objects (e.g. as entry function arguments or objects sent to addresses). For coin balances that need to be embedded in another Sui object struct, we use `Balance` instead because it's not a Sui object type and hence is much cheaper to use.
The wrapper object is then sent to the service operator, whose address is also specified in the call as `serviec_address`.

Although the service operator (`service_address`) now owns the `ObjectWrapper`, which contains the object to be swapped, there is nothing they can do about the object. They cannot transfer it even though we have defined a `transfer_object` entry function for `Object`. This is because they cannot pass the wrapped `Object` as an argument to the function.

Finally, let's define the function that the service operator can call in order to perform a swap between two objects sent from two accounts. The function interface will look like the following:
```rust
public(script) fun execute_swap(wrapper1: ObjectWrapper, wrapper2: ObjectWrapper, ctx: &mut TxContext);
```
where `wrapper1` and `wrapper2` are two wrapped objects that were sent from different objects owners to the service operator (hence the service operator owns both). Both wrapped objects are passed by value because they will eventually need to be unpacked.
We first check that the swap is indeed legit:
```rust
assert!(wrapper1.to_swap.scarcity == wrapper2.to_swap.scarcity, 0);
assert!(wrapper1.to_swap.style != wrapper2.to_swap.style, 0);
```
It checks that the two objects have identical scarcity, but have different style, perfect pair for a swap. Next we unpack the two objects to obtain the inner fields:
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
We now have all the things we need for the actual swap:
```rust
Transfer::transfer(object1, original_owner2);
Transfer::transfer(object2, original_owner1);
```
The above code does the swap: it sends `object1` to the original owner of `object2`, and sends `object1` to the original owner of `object2`. The service provider is also happy to take the fee:
```rust
let service_address = TxContext::sender(ctx);
Balance::join(&mut fee1, fee2);
Transfer::transfer(Coin::from_balance(fee1, ctx), service_address);
```
`fee2` is merged into `fee1`, turned into a `Coin` and sent to the `service_address`. Finally we signal Sui that we have deleted both wrapper objects:
```rust
ID::delete(id1);
ID::delete(id2);
```
At the end of this call, the two objects have been swapped (sent to the opposite owner) and the service provider takes the service fee.

Since the contract only defined one way to deal with `ObjectWrapper` which is `execute_swap`, there is no other way that the service operator can interact with `ObjectWrapper` even though they own them.

The full source code can be found in [TrustedSwap.move](../../../../sui_programmability/examples/objects_tutorial/sources/TrustedSwap.move).

A more complex example of using direct wrapping can be found in [Escrow.move](../../../../sui_programmability/examples/defi/sources/Escrow.move).

### Wrapping through `Option`
When Sui object type `Bar` is directly wrapped into `Foo`, there is not much flexiblity: a `Foo` object must has a `Bar` object in it, and in order to take out the `Bar` object one must destroy the `Foo` object. However, there are cases where we want more flexibility: the wrapping type may or may not always has the wrapped object in it, and the wrapped object may be replaced with a different object at some point.

Let's demonstrate this use case by designing a simple game character: A warrior with a sword and shield. A warrior may or may not have a sword and shield, and they should be able to replace them anytime. To design this, we define a `SimpleWarrior` type as following:
```rust
struct SimpleWarrior has key {
    id: VersionedID,
    sword: Option<Sword>,
    shield: Option<Shield>,
}
```
Each `SimpleWarrior` type has an optional `sword` and `shield` wrapped in it, defined as:
```rust
struct Sword has key, store {
    id: VersionedID,
    strength: u8,
}

struct Shield has key, store {
    id: VersionedID,
    armor: u8,
}
```
When we are creating a new warrior, we can set the `sword` and `shield` to none to indicate that there are no equipment yet:
```rust
public(script) fun create_warrior(ctx: &mut TxContext) {
    let warrior = SimpleWarrior {
        id: TxContext::new_id(ctx),
        sword: Option::none(),
        shield: Option::none(),
    };
    Transfer::transfer(warrior, TxContext::sender(ctx))
}
```
With this, we can then define functions to equip new swords or new shields:
```rust
public(script) fun equip_sword(warrior: &mut SimpleWarrior, sword: Sword, ctx: &mut TxContext) {
    if (Option::is_some(&warrior.sword)) {
        let old_sword = Option::extract(&mut warrior.sword);
        Transfer::transfer(old_sword, TxContext::sender(ctx));
    };
    Option::fill(&mut warrior.sword, sword);
}
```
In the above function, we are passing a `warrior` as mutable reference of `SimpleWarrior`, and a `sword` passed by value because we need to wrap it into the `warrior`.

It is important to note that because `Sword` is a Sui object type without `drop` ability, if the warrior already has a sword equipped, that sword cannot just be dropped. If we make a call to `Option::fill` without first checking and taking out the existig sword, a runtime error may happen. Hence in `equip_sword`, we first check if there is already a sword equipped, and if so, we take it out and send it back to the sender. This matches what you would typically expect when you equip a new sword: if a sword is already equipped, you get it back.

Full code can be found in [SimpleWarrior.move](../../../../sui_programmability/examples/objects_tutorial/sources/SimpleWarrior.move).

You can also find a more complex example in [Hero.move](../../../../sui_programmability/examples/games/hero/sources/Hero.move).

### Wrapping through `vector`
The concept of wrapping objects in a vector field of another Sui object is very similar to wrapping through `Option`: an object may contain 0, 1 or many of the wrapped objects of the same type.

To be finished.