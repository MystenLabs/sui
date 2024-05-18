# Address

`address` is a built-in type in Move that is used to represent locations (sometimes called accounts)
in storage. An `address` value is a 256-bit (32 byte) identifier. Move uses addresses to
differentiate packages of [modules](../modules.md), where each package has its own address and
modules. Specific deployments of Move might also use the `address` value for
[storage](../abilities.md#key) operations.

> For Sui, `address` is used to represent "accounts", and also objects via strong type wrappers
> (with `sui::object::UID` and `sui::object::ID`).

Although an `address` is a 256 bit integer under the hood, Move addresses are intentionally
opaque---they cannot be created from integers, they do not support arithmetic operations, and they
cannot be modified. Specific deployments of Move might have `native` functions to enable some of
these operations (e.g., creating an `address` from bytes `vector<u8>`), but these are not part of
the Move language itself.

While there are runtime address values (values of type `address`), they _cannot_ be used to access
modules at runtime.

## Addresses and Their Syntax

Addresses come in two flavors, named or numerical. The syntax for a named address follows the same
rules for any named identifier in Move. The syntax of a numerical address is not restricted to
hex-encoded values, and any valid [`u256` numerical value](./integers.md) can be used as an address
value, e.g., `42`, `0xCAFE`, and `10_000` are all valid numerical address literals.

To distinguish when an address is being used in an expression context or not, the syntax when using
an address differs depending on the context where it's used:

- When an address is used as an expression, the address must be prefixed by the `@` character, i.e.,
  [`@<numerical_value>`](./integers.md) or `@<named_address_identifier>`.
- Outside of expression contexts, the address may be written without the leading `@` character,
  i.e., [`<numerical_value>`](./integers.md) or `<named_address_identifier>`.

In general, you can think of `@` as an operator that takes an address from being a namespace item to
being an expression item.

## Named Addresses

Named addresses are a feature that allow identifiers to be used in place of numerical values in any
spot where addresses are used, and not just at the value level. Named addresses are declared and
bound as top level elements (outside of modules and scripts) in Move packages, or passed as
arguments to the Move compiler.

Named addresses only exist at the source language level and will be fully substituted for their
value at the bytecode level. Because of this, modules and module members should be accessed through
the module's named address and not through the numerical value assigned to the named address during
compilation. So while `use my_addr::foo` is equivalent to `use 0x2::foo` (if `my_addr` is assigned
`0x2`), it is a best practice to always use the `my_addr` name.

### Examples

```move
// shorthand for
// 0x0000000000000000000000000000000000000000000000000000000000000001
let a1: address = @0x1;
// shorthand for
// 0x0000000000000000000000000000000000000000000000000000000000000042
let a2: address = @0x42;
// shorthand for
// 0x00000000000000000000000000000000000000000000000000000000DEADBEEF
let a3: address = @0xDEADBEEF;
// shorthand for
// 0x000000000000000000000000000000000000000000000000000000000000000A
let a4: address = @0x0000000000000000000000000000000A;
// Assigns `a5` the value of the named address `std`
let a5: address = @std;
// Any valid numerical value can be used as an address
let a6: address = @66;
let a7: address = @42_000;

module 66::some_module {   // Not in expression context, so no @ needed
    use 0x1::other_module; // Not in expression context so no @ needed
    use std::vector;       // Can use a named address as a namespace item
    ...
}

module std::other_module {  // Can use a named address when declaring a module
    ...
}
```
