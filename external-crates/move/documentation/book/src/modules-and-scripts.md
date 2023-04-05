# Modules and Scripts

Move has two different types of programs: ***Modules*** and ***Scripts***. Modules are libraries that define struct types along with functions that operate on these types. Struct types define the schema of Move's [global storage](./global-storage-structure.md), and module functions define the rules for updating storage. Modules themselves are also stored in global storage. Scripts are executable entrypoints similar to a `main` function in a conventional language. A script typically calls functions of a published module that perform updates to global storage. Scripts are ephemeral code snippets that are not published in global storage.

A Move source file (or **compilation unit**) may contain multiple modules and scripts. However, publishing a module or executing a script are separate VM operations.

## Syntax

### Scripts

A script has the following structure:

```text
script {
    <use>*
    <constants>*
    fun <identifier><[type parameters: constraint]*>([identifier: type]*) <function_body>
}
```

A `script` block must start with all of its [`use`](./uses.md) declarations, followed by any [constants](./constants.md) and (finally) the main
[function](./functions.md) declaration.
The main function can have any name (i.e., it need not be called `main`), is the only function in a script block, can have any number of
arguments, and must not return a value. Here is an example with each of these components:

```move
script {
    // Import the debug module published at the named account address std.
    use std::debug;

    const ONE: u64 = 1;

    fun main(x: u64) {
        let sum = x + ONE;
        debug::print(&sum)
    }
}
```

Scripts have very limited power—they cannot declare friends, struct types or access global storage. Their primary purpose is to invoke module functions.

### Modules

A module has the following syntax:

```text
module <address>::<identifier> {
    (<use> | <friend> | <type> | <function> | <constant>)*
}
```

where `<address>` is a valid [named or literal address](./address.md).

For example:

```move
module 0x42::test {
    struct Example has copy, drop { i: u64 }

    use std::debug;
    friend 0x42::another_test;

    const ONE: u64 = 1;

    public fun print(x: u64) {
        let sum = x + ONE;
        let example = Example { i: sum };
        debug::print(&sum)
    }
}
```

The `module 0x42::test` part specifies that the module `test` will be published under the [account address](./address.md) `0x42` in [global storage](./global-storage-structure.md).

Modules can also be declared using [named addresses](./address.md). For example:

```move
module test_addr::test {
    struct Example has copy, drop { a: address }

    use std::debug;
    friend test_addr::another_test;

    public fun print() {
        let example = Example { a: @test_addr };
        debug::print(&example)
    }
}
```

Because named addresses only exist at the source language level and during compilation,
named addresses will be fully substituted for their value at the bytecode
level. For example if we had the following code:

```move
script {
    fun example() {
        my_addr::m::foo(@my_addr);
    }
}
```

and we compiled it with `my_addr` set to `0xC0FFEE`, then it would be equivalent
to the following operationally:

```move
script {
    fun example() {
        0xC0FFEE::m::foo(@0xC0FFEE);
    }
}
```

However at the source level, these _are not equivalent_—the function
`m::foo` _must_ be accessed through the `my_addr` named address, and not through
the numerical value assigned to that address.

Module names can start with letters `a` to `z` or letters `A` to `Z`. After the first character, module names can contain underscores `_`, letters `a` to `z`, letters `A` to `Z`, or digits `0` to `9`.

```move
module my_module {}
module foo_bar_42 {}
```

Typically, module names start with an lowercase letter. A module named `my_module` should be stored in a source file named `my_module.move`.

All elements inside a `module` block can appear in any order.
Fundamentally, a module is a collection of [`types`](./structs-and-resources.md) and [`functions`](./functions.md).
The [`use`](./uses.md) keyword is used to import types from other modules.
The [`friend`](./friends.md) keyword specifies a list of trusted modules.
The [`const`](./constants.md) keyword defines private constants that can be used in the functions of a module.
