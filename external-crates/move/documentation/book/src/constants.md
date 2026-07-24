# Constants

Constants are a way of giving a name to shared, static values inside of a `module`.

The constant's value must be known at compilation. The constant's value is stored in the compiled
module. And each time the constant is used, a new copy of that value is made.

## Declaration

Constant declarations begin with the `const` keyword, followed by a name, a type, and a value.

```text
const <name>: <type> = <expression>;
```

For example

```move
module a::example {
    const MY_ADDRESS: address = @a;

    public fun permissioned(addr: address) {
        assert!(addr == MY_ADDRESS, 0);
    }
}
```

## Naming

Constants must start with a capital letter `A` to `Z`. After the first letter, constant names can
contain underscores `_`, letters `a` to `z`, letters `A` to `Z`, or digits `0` to `9`.

```move
const FLAG: bool = false;
const EMyErrorCode: u64 = 0;
const ADDRESS_42: address = @0x42;
```

Even though you can use letters `a` to `z` in a constant. The
[general style guidelines](./coding-conventions.md) are to use just uppercase letters `A` to `Z`,
with underscores `_` between each word. For error codes, we use `E` as a prefix and then upper camel
case (also known as Pascal case) for the rest of the name, as seen in `EMyErrorCode`.

The current naming restriction of starting with `A` to `Z` is in place to give room for future
language features.

## Visibility

By default, `const` values can be used only in the declaring module. However, as a convenience,
they can be used across modules in [unit tests attributes](./unit-testing.md).

With the `2024.alpha` edition, a constant can be declared `public(package)`, which makes it
usable from any module in the *same package* as its declaring module. No other visibility
modifier is valid on a constant.

### Cross-Module Usage

A `public(package)` constant can be used from any module of its package, both in code and in
the definitions of other constants:

```move
module a::config {
    public(package) const MAX_SUPPLY: u64 = 1_000_000;
}

module a::mint {
    use a::config;

    // folded to a value at compile time
    const HALF_SUPPLY: u64 = config::MAX_SUPPLY / 2;

    public fun mint(amount: u64) {
        // compiled as a call into `a::config`
        assert!(amount <= config::MAX_SUPPLY, 0);
        // ...
    }
}
```

Even with `public(package)`, constants remain internal to their package: using a constant from
another package is an error.

The two kinds of usage behave differently when the defining package is upgraded:

- A cross-module use in a *constant definition* (like `HALF_SUPPLY` above) is resolved by the
  compiler, which folds the referenced constant's value into the new constant at compile time.
- A cross-module use in a *function body* compiles to a call of a `public(package)` function
  that the compiler generates in the defining module, so the value read at runtime is the one in
  the version of the defining module the code is linked against.

Within a single package version this distinction is unobservable, since all modules of a package
are compiled and published together. Note that reading a cross-module constant may incur slightly
more gas usage than reading a constant of the current module, as it is compiled as a function
call.

One usage is restricted: a [`#[error]` constant](./abort-and-assert.md) used as an abort code
must come from the aborting module, since its name and value are encoded against the aborting
module's tables.

## Valid Expressions

Currently, constants are limited to the primitive types `bool`, `u8`, `u16`, `u32`, `u64`, `u128`,
`u256`, `address`, and `vector<T>`, where `T` is the valid type for a constant.

### Values

Commonly, `const`s are assigned a simple value, or literal, of their type. For example

```move
const MY_BOOL: bool = false;
const MY_ADDRESS: address = @0x70DD;
const BYTES: vector<u8> = b"hello world";
const HEX_BYTES: vector<u8> = x"DEADBEEF";
```

### Complex Expressions

In addition to literals, constants can include more complex expressions, as long as the compiler is
able to reduce the expression to a value at compile time.

Currently, equality operations, all boolean operations, all bitwise operations, and all arithmetic
operations can be used.

```move
const RULE: bool = true && false;
const CAP: u64 = 10 * 100 + 1;
const SHIFTY: u8 = {
    (1 << 1) * (1 << 2) * (1 << 3) * (1 << 4)
};
const HALF_MAX: u128 = 340282366920938463463374607431768211455 / 2;
const REM: u256 =
    57896044618658097711785492504343953926634992332820282019728792003956564819968 % 654321;
const EQUAL: bool = 1 == 1;
```

If the operation would result in a runtime exception, the compiler will give an error that it is
unable to generate the constant's value

```move
const DIV_BY_ZERO: u64 = 1 / 0; // ERROR!
const SHIFT_BY_A_LOT: u64 = 1 << 100; // ERROR!
const NEGATIVE_U64: u64 = 0 - 1; // ERROR!
```

Additionally, constants can refer to other constants within the same module (or, with the
`2024.alpha` edition, to `public(package)` constants of other modules in the same package, as
described in [Cross-Module Usage](#cross-module-usage)).

```move
const BASE: u8 = 4;
const SQUARE: u8 = BASE * BASE;
```

Note though, that any cycle in the constant definitions results in an error. This includes
cycles formed across modules, which are reported as module dependency cycles.

```move
const A: u16 = B + 1;
const B: u16 = A + 1; // ERROR!
```
