# Abilities

Abilities are a typing feature in Move that control what actions are permissible for values of a
given type. This system grants fine grained control over the "linear" typing behavior of values, as
well as if and how values are used in storage (as defined by the specific deployment of Move, e.g.
the notion of storage for the blockchain). This is implemented by gating access to certain bytecode
instructions so that for a value to be used with the bytecode instruction, it must have the ability
required (if one is required at all—not every instruction is gated by an ability).

For Sui, `key` is used to signify an [object](./abilities/object.md). Objects are the basic unit of
storage where each object has a unique, 32-byte ID. `store` is then used to both indicate what data
can be stored inside of an object, and is also used to indicate what types can be transferred
outside of their defining module.

<!-- TODO future section on detailed walk through maybe. We have some examples at the end but it might be helpful to explain why we have precisely this set of abilities

If you are already somewhat familiar with abilities from writing Move programs, but are still confused as to what is going on, it might be helpful to skip to the [motivating walkthrough](#motivating-walkthrough) section to get an idea of what the system is setup in the way that it is. -->

## The Four Abilities

The four abilities are:

- [`copy`](#copy)
  - Allows values of types with this ability to be copied.
- [`drop`](#drop)
  - Allows values of types with this ability to be popped/dropped.
- [`store`](#store)
  - Allows values of types with this ability to exist inside a value in storage.
  - For Sui, `store` controls what data can be stored inside of an [object](./abilities/object.md).
    `store` also controls what types can be transferred outside of their defining module.
- [`key`](#key)
  - Allows the type to serve as a "key" for storage. Ostensibly this means the value can be a
    top-level value in storage; in other words, it does not need to be contained in another value to
    be in storage.
  - For Sui, `key` is used to signify an [object](./abilities/object.md).

### `copy`

The `copy` ability allows values of types with that ability to be copied. It gates the ability to
copy values out of local variables with the [`copy`](./variables.md#move-and-copy) operator and to
copy values via references with
[dereference `*e`](./primitive-types/references.md#reading-and-writing-through-references).

If a value has `copy`, all values contained inside of that value have `copy`.

### `drop`

The `drop` ability allows values of types with that ability to be dropped. By dropped, we mean that
value is not transferred and is effectively destroyed as the Move program executes. As such, this
ability gates the ability to ignore values in a multitude of locations, including:

- not using the value in a local variable or parameter
- not using the value in a [sequence via `;`](./variables.md#expression-blocks)
- overwriting values in variables in [assignments](./variables.md#assignments)
- overwriting values via references when
  [writing `*e1 = e2`](./primitive-types/references.md#reading-and-writing-through-references).

If a value has `drop`, all values contained inside of that value have `drop`.

### `store`

The `store` ability allows values of types with this ability to exist inside of a value in storage,
_but_ not necessarily as a top-level value in storage. This is the only ability that does not
directly gate an operation. Instead it gates the existence in storage when used in tandem with
`key`.

If a value has `store`, all values contained inside of that value have `store`.

For Sui, `store` serves double duty. It controls what values can appear inside of an
[object](./abilities/object.md), and what objects can be
[transferred](./abilities/object.md#transfer-rules) outside of their defining module.

### `key`

The `key` ability allows the type to serve as a key for storage operations as defined by the
deployment of Move. While it is specific per Move instance, it serves to gates all storage
operations, so in order for a type to be used with storage primitives, the type must have the `key`
ability.

If a value has `key`, all values contained inside of that value have `store`. This is the only
ability with this sort of asymmetry.

For Sui, `key` is used to signify an [object](./abilities/object.md).

## Builtin Types

All primitive, builtin types have `copy`, `drop`, and `store`.

- `bool`, `u8`, `u16`, `u32`, `u64`, `u128`, `u256`, and `address` all have `copy`, `drop`, and
  `store`.
- `vector<T>` may have `copy`, `drop`, and `store` depending on the abilities of `T`.
  - See [Conditional Abilities and Generic Types](#conditional-abilities-and-generic-types) for more
    details.
- Immutable references `&` and mutable references `&mut` both have `copy` and `drop`.
  - This refers to copying and dropping the reference itself, not what they refer to.
  - References cannot appear in global storage, hence they do not have `store`.

Note that none of the primitive types have `key`, meaning none of them can be used directly with
storage operations.

## Annotating Structs

To declare that a `struct` has an ability, it is declared with `has <ability>` after the struct name
and either before or after the fields. For example:

```move
public struct Ignorable has drop { f: u64 }
public struct Pair has copy, drop, store { x: u64, y: u64 }
public struct MyVec(vector<u64>) has copy, drop, store;
```

In this case: `Ignorable` has the `drop` ability. `Pair` and `MyVec` both have `copy`, `drop`, and
`store`.

All of these abilities have strong guarantees over these gated operations. The operation can be
performed on the value only if it has that ability; even if the value is deeply nested inside of
some other collection!

As such: when declaring a struct’s abilities, certain requirements are placed on the fields. All
fields must satisfy these constraints. These rules are necessary so that structs satisfy the
reachability rules for the abilities given above. If a struct is declared with the ability...

- `copy`, all fields must have `copy`.
- `drop`, all fields must have `drop`.
- `store`, all fields must have `store`.
- `key`, all fields must have `store`.
  - `key` is the only ability currently that doesn’t require itself.

For example:

```move
// A struct without any abilities
public struct NoAbilities {}

public struct WantsCopy has copy {
    f: NoAbilities, // ERROR 'NoAbilities' does not have 'copy'
}
```

and similarly:

```move
// A struct without any abilities
public struct NoAbilities {}

public struct MyData has key {
    f: NoAbilities, // Error 'NoAbilities' does not have 'store'
}
```

## Conditional Abilities and Generic Types

When abilities are annotated on a generic type, not all instances of that type are guaranteed to
have that ability. Consider this struct declaration:

```move
public struct Cup<T> has copy, drop, store, key { item: T }
```

It might be very helpful if `Cup` could hold any type, regardless of its abilities. The type system
can _see_ the type parameter, so it should be able to remove abilities from `Cup` if it _sees_ a
type parameter that would violate the guarantees for that ability.

This behavior might sound a bit confusing at first, but it might be more understandable if we think
about collection types. We could consider the builtin type `vector` to have the following type
declaration:

```move
vector<T> has copy, drop, store;
```

We want `vector`s to work with any type. We don't want separate `vector` types for different
abilities. So what are the rules we would want? Precisely the same that we would want with the field
rules above. So, it would be safe to copy a `vector` value only if the inner elements can be copied.
It would be safe to ignore a `vector` value only if the inner elements can be ignored/dropped. And,
it would be safe to put a `vector` in storage only if the inner elements can be in storage.

To have this extra expressiveness, a type might not have all the abilities it was declared with
depending on the instantiation of that type; instead, the abilities a type will have depends on both
its declaration **and** its type arguments. For any type, type parameters are pessimistically
assumed to be used inside of the struct, so the abilities are only granted if the type parameters
meet the requirements described above for fields. Taking `Cup` from above as an example:

- `Cup` has the ability `copy` only if `T` has `copy`.
- It has `drop` only if `T` has `drop`.
- It has `store` only if `T` has `store`.
- It has `key` only if `T` has `store`.

Here are examples for this conditional system for each ability:

### Example: conditional `copy`

```move
public struct NoAbilities {}
public struct S has copy, drop { f: bool }
public struct Cup<T> has copy, drop, store { item: T }

fun example(c_x: Cup<u64>, c_s: Cup<S>) {
    // Valid, 'Cup<u64>' has 'copy' because 'u64' has 'copy'
    let c_x2 = copy c_x;
    // Valid, 'Cup<S>' has 'copy' because 'S' has 'copy'
    let c_s2 = copy c_s;
}

fun invalid(c_account: Cup<signer>, c_n: Cup<NoAbilities>) {
    // Invalid, 'Cup<signer>' does not have 'copy'.
    // Even though 'Cup' was declared with copy, the instance does not have 'copy'
    // because 'signer' does not have 'copy'
    let c_account2 = copy c_account;
    // Invalid, 'Cup<NoAbilities>' does not have 'copy'
    // because 'NoAbilities' does not have 'copy'
    let c_n2 = copy c_n;
}
```

### Example: conditional `drop`

```move
public struct NoAbilities {}
public struct S has copy, drop { f: bool }
public struct Cup<T> has copy, drop, store { item: T }

fun unused() {
    Cup<bool> { item: true }; // Valid, 'Cup<bool>' has 'drop'
    Cup<S> { item: S { f: false }}; // Valid, 'Cup<S>' has 'drop'
}

fun left_in_local(c_account: Cup<signer>): u64 {
    let c_b = Cup<bool> { item: true };
    let c_s = Cup<S> { item: S { f: false }};
    // Valid return: 'c_account', 'c_b', and 'c_s' have values
    // but 'Cup<signer>', 'Cup<bool>', and 'Cup<S>' have 'drop'
    0
}

fun invalid_unused() {
    // Invalid, Cannot ignore 'Cup<NoAbilities>' because it does not have 'drop'.
    // Even though 'Cup' was declared with 'drop', the instance does not have 'drop'
    // because 'NoAbilities' does not have 'drop'
    Cup<NoAbilities> { item: NoAbilities {} };
}

fun invalid_left_in_local(): u64 {
    let n = Cup<NoAbilities> { item: NoAbilities {} };
    // Invalid return: 'c_n' has a value
    // and 'Cup<NoAbilities>' does not have 'drop'
    0
}
```

### Example: conditional `store`

```move
public struct Cup<T> has copy, drop, store { item: T }

// 'MyInnerData is declared with 'store' so all fields need 'store'
struct MyInnerData has store {
    yes: Cup<u64>, // Valid, 'Cup<u64>' has 'store'
    // no: Cup<signer>, Invalid, 'Cup<signer>' does not have 'store'
}

// 'MyData' is declared with 'key' so all fields need 'store'
struct MyData has key {
    yes: Cup<u64>, // Valid, 'Cup<u64>' has 'store'
    inner: Cup<MyInnerData>, // Valid, 'Cup<MyInnerData>' has 'store'
    // no: Cup<signer>, Invalid, 'Cup<signer>' does not have 'store'
}
```

### Example: conditional `key`

```move
public struct NoAbilities {}
public struct MyData<T> has key { f: T }

fun valid(addr: address) acquires MyData {
    // Valid, 'MyData<u64>' has 'key'
    transfer(addr, MyData<u64> { f: 0 });
}

fun invalid(addr: address) {
   // Invalid, 'MyData<NoAbilities>' does not have 'key'
   transfer(addr, MyData<NoAbilities> { f: NoAbilities {} })
   // Invalid, 'MyData<NoAbilities>' does not have 'key'
   borrow<NoAbilities>(addr);
   // Invalid, 'MyData<NoAbilities>' does not have 'key'
   borrow_mut<NoAbilities>(addr);
}

// Mock storage operation
native public fun transfer<T: key>(addr: address, value: T);
```
