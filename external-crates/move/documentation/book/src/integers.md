# Integers

Move supports six unsigned integer types: `u8`, `u16`, `u32`, `u64`, `u128`, and `u256`. Values of these types range from 0 to a maximum that depends on the size of the type.

| Type                             | Value Range              |
| -------------------------------- | ------------------------ |
| Unsigned 8-bit integer, `u8`     | 0 to 2<sup>8</sup> - 1   |
| Unsigned 16-bit integer, `u16`   | 0 to 2<sup>16</sup> - 1  |
| Unsigned 32-bit integer, `u32`   | 0 to 2<sup>32</sup> - 1  |
| Unsigned 64-bit integer, `u64`   | 0 to 2<sup>64</sup> - 1  |
| Unsigned 128-bit integer, `u128` | 0 to 2<sup>128</sup> - 1 |
| Unsigned 256-bit integer, `u256` | 0 to 2<sup>256</sup> - 1 |

## Literals

Literal values for these types are specified either as a sequence of digits (e.g.,`112`) or as hex literals, e.g., `0xFF`. The type of the literal can optionally be added as a suffix, e.g., `112u8`. If the type is not specified, the compiler will try to infer the type from the context where the literal is used. If the type cannot be inferred, it is assumed to be `u64`.

Number literals can be separated by underscores for grouping and readability. (e.g.,`1_234_5678`, `1_000u128`, `0xAB_CD_12_35`).

If a literal is too large for its specified (or inferred) size range, an error is reported.

### Examples

```move
// literals with explicit annotations;
let explicit_u8 = 1u8;
let explicit_u16 = 1u16;
let explicit_u32 = 1u32;
let explicit_u64 = 2u64;
let explicit_u128 = 3u128;
let explicit_u256 = 1u256;
let explicit_u64_underscored = 154_322_973u64;

// literals with simple inference
let simple_u8: u8 = 1;
let simple_u16: u16 = 1;
let simple_u32: u32 = 1;
let simple_u64: u64 = 2;
let simple_u128: u128 = 3;
let simple_u256: u256 = 1;

// literals with more complex inference
let complex_u8 = 1; // inferred: u8
// right hand argument to shift must be u8
let _unused = 10 << complex_u8;

let x: u8 = 38;
let complex_u8 = 2; // inferred: u8
// arguments to `+` must have the same type
let _unused = x + complex_u8;

let complex_u128 = 133_876; // inferred: u128
// inferred from function argument type
function_that_takes_u128(complex_u128);

// literals can be written in hex
let hex_u8: u8 = 0x1;
let hex_u16: u16 = 0x1BAE;
let hex_u32: u32 = 0xDEAD80;
let hex_u64: u64 = 0xCAFE;
let hex_u128: u128 = 0xDEADBEEF;
let hex_u256: u256 = 0x1123_456A_BCDE_F;
```

## Operations

### Arithmetic

Each of these types supports the same set of checked arithmetic operations. For all of these operations, both arguments (the left and right side operands) *must* be of the same type. If you need to operate over values of different types, you will need to first perform a [cast](#casting). Similarly, if you expect the result of the operation to be too large for the integer type, perform a [cast](#casting) to a larger size before performing the operation.

All arithmetic operations abort instead of behaving in a way that mathematical integers would not (e.g., overflow, underflow, divide-by-zero).

| Syntax | Operation | Aborts If
|--------|-----------|-------------------------------------
| `+` |addition | Result is too large for the integer type
| `-` | subtraction | Result is less than zero
| `*` | multiplication | Result is too large for the integer type
| `%` | modular division | The divisor is `0`
| `/` | truncating division | The divisor is `0`

### Bitwise

The integer types support the following bitwise operations that treat each number as a series of individual bits, either 0 or 1, instead of as numerical integer values.

Bitwise operations do not abort.

| Syntax              | Operation   | Description                                           |
|---------------------|-------------|-------------------------------------------------------|
| `&`                 | bitwise and | Performs a boolean and for each bit pairwise          |
| <code>&#124;</code> | bitwise or  | Performs a boolean or for each bit pairwise           |
| `^`                 | bitwise xor | Performs a boolean exclusive or for each bit pairwise |

### Bit Shifts

Similar to the bitwise operations, each integer type supports bit shifts. But unlike the other operations, the righthand side operand (how many bits to shift by) must *always* be a `u8` and need not match the left side operand (the number you are shifting).

Bit shifts can abort if the number of bits to shift by is greater than or equal to `8`, `16`, `32`, `64`, `128` or `256` for `u8`, `u16`, `u32`, `u64`, `u128` and `u256` respectively.

| Syntax | Operation  | Aborts if
|--------|------------|----------
|`<<`    | shift left | Number of bits to shift by is greater than the size of the integer type
|`>>`    | shift right| Number of bits to shift by is greater than the size of the integer type

### Comparisons

Integer types are the *only* types in Move that can use the comparison operators. Both arguments need to be of the same type. If you need to compare integers of different types, you will need to [cast](#casting) one of them first.

Comparison operations do not abort.

| Syntax | Operation
|--------|-----------
| `<`    | less than
| `>`    | greater than
| `<=`   | less than or equal to
| `>=`   | greater than or equal to

### Equality

Like all types with [`drop`](./abilities.md) in Move, all integer types support the ["equal"](./equality.md) and ["not equal"](./equality.md)  operations. Both arguments need to be of the same type. If you need to compare integers of different types, you will need to [cast](#casting) one of them first.

Equality operations do not abort.

| Syntax | Operation
|--------|----------
| `==`   | equal
| `!=`   | not equal

For more details see the section on [equality](./equality.md)

## Casting

Integer types of one size can be cast to integer types of another size. Integers are the only types in Move that support casting.

Casts *do not* truncate. Casting will abort if the result is too large for the specified type

| Syntax     | Operation                                                                       | Aborts if
|------------|---------------------------------------------------------------------------------|---------------------------------------
| `(e as T)`| Cast integer expression `e` into an integer type `T` | `e` is too large to represent as a `T`

Here, the type of `e` must be `8`, `16`, `32`, `64`, `128` or `256` and `T` must be `u8`, `u16`, `u32`, `u64`, `u128` oe `u256`.

For example:

- `(x as u8)`
- `(y as u16)`
- `(873u16 as u32)`
- `(2u8 as u64)`
- `(1 + 3 as u128)`
- `(4/2 + 12345 as u256)`

## Ownership

As with the other scalar values built-in to the language, integer values are implicitly copyable, meaning they can be copied without an explicit instruction such as [`copy`](./variables.md#move-and-copy).
