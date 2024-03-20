# Address

`address` is a built-in type in Move that is used to represent locations (sometimes called accounts) in global storage. An `address` value is a 128-bit (16 byte) identifier. At a given address, two things can be stored: [Modules](./modules-and-scripts.md) and [Resources](./structs-and-resources.md).

Although an `address` is a 128 bit integer under the hood, Move addresses are intentionally opaque---they cannot be created from integers, they do not support arithmetic operations, and they cannot be modified. Even though there might be interesting programs that would use such a feature (e.g., pointer arithmetic in C fills a similar niche), Move does not allow this dynamic behavior because it has been designed from the ground up to support static verification.

You can use runtime address values (values of type `address`) to access resources at that address. You *cannot* access modules at runtime via address values.

## Addresses and Their Syntax

Addresses come in two flavors, named or numerical. The syntax for a named address follows the
same rules for any named identifier in Move. The syntax of a numerical address is not restricted
to hex-encoded values, and any valid [`u128` numerical value](./integers.md) can be used as an
address value, e.g., `42`, `0xCAFE`, and `2021` are all valid numerical address
literals.

To distinguish when an address is being used in an expression context or not, the
syntax when using an address differs depending on the context where it's used:
* When an address is used as an expression the address must be prefixed by the `@` character, i.e., [`@<numerical_value>`](./integers.md) or `@<named_address_identifier>`.
* Outside of expression contexts, the address may be written without the leading `@` character, i.e., [`<numerical_value>`](./integers.md) or `<named_address_identifier>`.

In general, you can think of `@` as an operator that takes an address from being a namespace item to being an expression item.

## Named Addresses

Named addresses are a feature that allow identifiers to be used in place of
numerical values in any spot where addresses are used, and not just at the
value level.  Named addresses are declared and bound as top level elements
(outside of modules and scripts) in Move Packages, or passed as arguments
to the Move compiler.

Named addresses only exist at the source language level and will be fully
substituted for their value at the bytecode level. Because of this, modules
and module members _must_ be accessed through the module's named address
and not through the numerical value assigned to the named address during
compilation, e.g., `use my_addr::foo` is _not_ equivalent to `use 0x2::foo`
even if the Move program is compiled with `my_addr` set to `0x2`. This
distinction is discussed in more detail in the section on [Modules and
Scripts](./modules-and-scripts.md).

### Examples

```move
let a1: address = @0x1; // shorthand for 0x00000000000000000000000000000001
let a2: address = @0x42; // shorthand for 0x00000000000000000000000000000042
let a3: address = @0xDEADBEEF; // shorthand for 0x000000000000000000000000DEADBEEF
let a4: address = @0x0000000000000000000000000000000A;
let a5: address = @std; // Assigns `a5` the value of the named address `std`
let a6: address = @66;
let a7: address = @0x42;

module 66::some_module {   // Not in expression context, so no @ needed
    use 0x1::other_module; // Not in expression context so no @ needed
    use std::vector;       // Can use a named address as a namespace item when using other modules
    ...
}

module std::other_module {  // Can use a named address as a namespace item to declare a module
    ...
}
```

## Global Storage Operations

The primary purpose of `address` values are to interact with the global storage operations.

`address` values are used with the `exists`, `borrow_global`, `borrow_global_mut`, and `move_from` [operations](./global-storage-operators.md).

The only global storage operation that *does not* use `address` is `move_to`, which uses [`signer`](./signer.md).

## Ownership

As with the other scalar values built-in to the language, `address` values are implicitly copyable, meaning they can be copied without an explicit instruction such as [`copy`](./variables.md#move-and-copy).
