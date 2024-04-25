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

`public` or `public(package)` constants are not currently supported. `const` values can be used only
in the declaring module. However, as a convenience, they can be used across modules in
[unit tests attributes](./unit-testing.md).

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

Additionally, constants can refer to other constants within the same module.

```move
const BASE: u8 = 4;
const SQUARE: u8 = BASE * BASE;
```

Note though, that any cycle in the constant definitions results in an error.

```move
const A: u16 = B + 1;
const B: u16 = A + 1; // ERROR!
```
