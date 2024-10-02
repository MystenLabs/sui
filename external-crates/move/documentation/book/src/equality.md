# Equality

Move supports two equality operations `==` and `!=`

## Operations

| Syntax | Operation | Description                                                                 |
| ------ | --------- | --------------------------------------------------------------------------- |
| `==`   | equal     | Returns `true` if the two operands have the same value, `false` otherwise   |
| `!=`   | not equal | Returns `true` if the two operands have different values, `false` otherwise |

### Typing

Both the equal (`==`) and not-equal (`!=`) operations only work if both operands are the same type

```move
0 == 0; // `true`
1u128 == 2u128; // `false`
b"hello" != x"00"; // `true`
```

Equality and non-equality also work over _all_ user defined types!

```move=
module 0x42::example {
    public struct S has copy, drop { f: u64, s: vector<u8> }

    fun always_true(): bool {
        let s = S { f: 0, s: b"" };
        s == s
    }

    fun always_false(): bool {
        let s = S { f: 0, s: b"" };
        s != s
    }
}
```

If the operands have different types, there is a type checking error

```move
1u8 == 1u128; // ERROR!
//     ^^^^^ expected an argument of type 'u8'
b"" != 0; // ERROR!
//     ^ expected an argument of type 'vector<u8>'
```

### Typing with references

When comparing [references](./primitive-types/references.md), the type of the reference (immutable
or mutable) does not matter. This means that you can compare an immutable `&` reference with a
mutable one `&mut` of the same underlying type.

```move
let i = &0;
let m = &mut 1;

i == m; // `false`
m == i; // `false`
m == m; // `true`
i == i; // `true`
```

The above is equivalent to applying an explicit freeze to each mutable reference where needed

```move
let i = &0;
let m = &mut 1;

i == freeze(m); // `false`
freeze(m) == i; // `false`
m == m; // `true`
i == i; // `true`
```

But again, the underlying type must be the same type

```move
let i = &0;
let s = &b"";

i == s; // ERROR!
//   ^ expected an argument of type '&u64'
```

### Automatic Borrowing

Starting in Move 2024 edition, the `==` and `!=` operators automatically borrow their operands if
one of the operands is a reference and the other is not. This means that the following code works
without any errors:

```move
let r = &0;

// In all cases, `0` is automatically borrowed as `&0`
r == 0; // `true`
0 == r; // `true`
r != 0; // `false`
0 != r; // `false`
```

This automatic borrow is always an immutable borrow.

## Restrictions

Both `==` and `!=` consume the value when comparing them. As a result, the type system enforces that
the type must have [`drop`](./abilities.md). Recall that without the
[`drop` ability](./abilities.md), ownership must be transferred by the end of the function, and such
values can only be explicitly destroyed within their declaring module. If these were used directly
with either equality `==` or non-equality `!=`, the value would be destroyed which would break
[`drop` ability](./abilities.md) safety guarantees!

```move=
module 0x42::example {
    public struct Coin has store { value: u64 }
    fun invalid(c1: Coin, c2: Coin) {
        c1 == c2 // ERROR!
//      ^^    ^^ These assets would be destroyed!
    }
}
```

But, a programmer can _always_ borrow the value first instead of directly comparing the value, and
reference types have the [`drop` ability](./abilities.md). For example

```move=
module 0x42::example {
    public struct Coin as store { value: u64 }
    fun swap_if_equal(c1: Coin, c2: Coin): (Coin, Coin) {
        let are_equal = &c1 == c2; // valid, note `c2` is automatically borrowed
        if (are_equal) (c2, c1) else (c1, c2)
    }
}
```

## Avoid Extra Copies

While a programmer _can_ compare any value whose type has [`drop`](./abilities.md), a programmer
should often compare by reference to avoid expensive copies.

```move=
let v1: vector<u8> = function_that_returns_vector();
let v2: vector<u8> = function_that_returns_vector();
assert!(copy v1 == copy v2, 42);
//      ^^^^       ^^^^
use_two_vectors(v1, v2);

let s1: Foo = function_that_returns_large_struct();
let s2: Foo = function_that_returns_large_struct();
assert!(copy s1 == copy s2, 42);
//      ^^^^       ^^^^
use_two_foos(s1, s2);
```

This code is perfectly acceptable (assuming `Foo` has [`drop`](./abilities.md)), just not efficient.
The highlighted copies can be removed and replaced with borrows

```move=
let v1: vector<u8> = function_that_returns_vector();
let v2: vector<u8> = function_that_returns_vector();
assert!(&v1 == &v2, 42);
//      ^      ^
use_two_vectors(v1, v2);

let s1: Foo = function_that_returns_large_struct();
let s2: Foo = function_that_returns_large_struct();
assert!(&s1 == &s2, 42);
//      ^      ^
use_two_foos(s1, s2);
```

The efficiency of the `==` itself remains the same, but the `copy`s are removed and thus the program
is more efficient.
