# Modules

**Modules** are the core program unit that define types along with functions that operate on these
types. Struct types define the schema of Move's [storage](./abilities.md#key), and module functions
define the rules interacting with values of those types. While modules themselves are also stored in
storage, they are not accessible from within a Move program. In a blockchain environment, the
modules are stored on chain in a process typically referred to as "publishing". After being
published, [`entry`](./functions.md#entry-modifier) and [`public`](./functions.md#visibility)
functions can be invoked according to the rules of that particular Move instance.

## Syntax

A module has the following syntax:

```text
module <address>::<identifier> {
    (<use> | <type> | <function> | <constant>)*
}
```

where `<address>` is a valid [address](./primitive-types/address.md) specifying the module's
package.

For example:

```move
module 0x42::test {
    public struct Example has copy, drop { i: u64 }

    use std::debug;

    const ONE: u64 = 1;

    public fun print(x: u64) {
        let sum = x + ONE;
        let example = Example { i: sum };
        debug::print(&sum)
    }
}
```

## Names

The `module test_addr::test` part specifies that the module `test` will be published under the
numerical [address](./primitive-types/address.md) value assigned for the name `test_addr` in the
[package settings](./packages.md).

Modules should normally be declared using [named addresses](./primitive-types/address.md) (as
opposed to using the numerical value directly). For example:

```move
module test_addr::test {
    public struct Example has copy, drop { a: address }

    friend test_addr::another_test;

    public fun print() {
        let example = Example { a: @test_addr };
        debug::print(&example)
    }
}
```

These named addresses commonly match the name of the [package](./packages.md).

Because named addresses only exist at the source language level and during compilation, named
addresses will be fully substituted for their value at the bytecode level. For example if we had the
following code:

```move
fun example() {
    my_addr::m::foo(@my_addr);
}
```

and we compiled it with `my_addr` set to `0xC0FFEE`, then it would be operationally equivalent to
the following:

```move
fun example() {
    0xC0FFEE::m::foo(@0xC0FFEE);
}
```

While at the source level these two different accesses are equivalent, it is a best practice to
always use the named address and not the numerical value assigned to that address.

Module names can start with a lowercase letter from `a` to `z` or an uppercase letter from `A` to
`Z`. After the first character, module names can contain underscores `_`, letters `a` to `z`,
letters `A` to `Z`, or digits `0` to `9`.

```move
module a::my_module {}
module a::foo_bar_42 {}
```

Typically, module names start with a lowercase letter. A module named `my_module` should be stored
in a source file named `my_module.move`.

## Members

All members inside a `module` block can appear in any order. Fundamentally, a module is a collection
of [`types`](./structs.md) and [`functions`](./functions.md). The [`use`](./uses.md) keyword refers
to members from other modules. The [`const`](./constants.md) keyword defines constants that can be
used in the functions of a module.

The [`friend`](./friends.md) syntax is a deprecated concept for specifying a list of trusted
modules. The concept has been superceded by [`public(package)`](./functions.md#visibility)

<!-- TODO member access rules -->
