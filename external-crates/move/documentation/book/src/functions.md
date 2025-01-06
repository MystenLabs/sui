# Functions

Functions are declared inside of modules and define the logic and behavior of the module. Functions
can be reused, either being called from other functions or as entry points for execution.

## Declaration

Functions are declared with the `fun` keyword followed by the function name, type parameters,
parameters, a return type, and finally the function body.

```text
<visibility>? <entry>? fun <identifier><[type_parameters: constraint],*>([identifier: type],*): <return_type> <function_body>
```

For example

```move
fun foo<T1, T2>(x: u64, y: T1, z: T2): (T2, T1, u64) { (z, y, x) }
```

### Visibility

Module functions, by default, can only be called within the same module. These internal (sometimes
called private) functions cannot be called from other modules or as entry points.

```move
module a::m {
    fun foo(): u64 { 0 }
    fun calls_foo(): u64 { foo() } // valid
}

module b::other {
    fun calls_m_foo(): u64 {
        a::m::foo() // ERROR!
//      ^^^^^^^^^^^ 'foo' is internal to 'a::m'
    }
}
```

To allow access from other modules, the function must be declared `public` or `public(package)`.
Tangential to visibility, an [`entry`](#entry-modifier) function can be called as an entry point for
execution.

#### `public` visibility

A `public` function can be called by _any_ function defined in _any_ module. As shown in the
following example, a `public` function can be called by:

- other functions defined in the same module,
- functions defined in another module, or
- as an entry point for execution.

```move
module a::m {
    public fun foo(): u64 { 0 }
    fun calls_foo(): u64 { foo() } // valid
}

module b::other {
    fun calls_m_foo(): u64 {
        a::m::foo() // valid
    }
}
```

Fore more details on the entry point to execution see [the section below](#entry-modifier).

#### `public(package)` visibility

The `public(package)` visibility modifier is a more restricted form of the `public` modifier to give
more control about where a function can be used. A `public(package)` function can be called by:

- other functions defined in the same module, or
- other functions defined in the same package (the same address)

```move
module a::m {
    public(package) fun foo(): u64 { 0 }
    fun calls_foo(): u64 { foo() } // valid
}

module a::n {
    fun calls_m_foo(): u64 {
        a::m::foo() // valid, also in `a`
    }
}

module b::other {
    fun calls_m_foo(): u64 {
        b::m::foo() // ERROR!
//      ^^^^^^^^^^^ 'foo' can only be called from a module in `a`
    }
}
```

#### DEPRECATED `public(friend)` visibility

Before the addition of `public(package)`, `public(friend)` was used to allow limited public access
to functions in the same package, but where the list of allowed modules had to be explicitly
enumerated by the callee's module. see [Friends](./friends.md) for more details.

### `entry` modifier

In addition to `public` functions, you might have some functions in your modules that you want to
use as the entry point to execution. The `entry` modifier is designed to allow module functions to
initiate execution, without having to expose the functionality to other modules.

Essentially, the combination of `pbulic` and `entry` functions define the "main" functions of a
module, and they specify where Move programs can start executing.

Keep in mind though, an `entry` function _can_ still be called by other Move functions. So while
they _can_ serve as the start of a Move program, they aren't restricted to that case.

For example:

```move
module a::m {
    entry fun foo(): u64 { 0 }
    fun calls_foo(): u64 { foo() } // valid!
}

module a::n {
    fun calls_m_foo(): u64 {
        a::m::foo() // ERROR!
//      ^^^^^^^^^^^ 'foo' is internal to 'a::m'
    }
}
```

`entry` functions may have restrictions on their parameters and return types. Although, these
restrictions are specific to each individual deployment of Move.

[The documentation for `entry` functions on Sui can be found here.](https://docs.sui.io/concepts/sui-move-concepts/entry-functions).

### Name

Function names can start with letters `a` to `z`. After the first character, function names can
contain underscores `_`, letters `a` to `z`, letters `A` to `Z`, or digits `0` to `9`.

```move
fun fOO() {}
fun bar_42() {}
fun bAZ_19() {}
```

### Type Parameters

After the name, functions can have type parameters

```move
fun id<T>(x: T): T { x }
fun example<T1: copy, T2>(x: T1, y: T2): (T1, T1, T2) { (copy x, x, y) }
```

For more details, see [Move generics](./generics.md).

### Parameters

Functions parameters are declared with a local variable name followed by a type annotation

```move
fun add(x: u64, y: u64): u64 { x + y }
```

We read this as `x` has type `u64`

A function does not have to have any parameters at all.

```move
fun useless() { }
```

This is very common for functions that create new or empty data structures

```move
module a::example {
  public struct Counter { count: u64 }

  fun new_counter(): Counter {
      Counter { count: 0 }
  }
}
```

### Return type

After the parameters, a function specifies its return type.

```move
fun zero(): u64 { 0 }
```

Here `: u64` indicates that the function's return type is `u64`.

Using [tuples](./primitive-types/tuples.md), a function can return multiple values:

```move
fun one_two_three(): (u64, u64, u64) { (0, 1, 2) }
```

If no return type is specified, the function has an implicit return type of unit `()`. These
functions are equivalent:

```move
fun just_unit(): () { () }
fun just_unit() { () }
fun just_unit() { }
```

As mentioned in the [tuples section](./primitive-types/tuples.md), these tuple "values" do not exist
as runtime values. This means that a function that returns unit `()` does not return any value
during execution.

### Function body

A function's body is an expression block. The return value of the function is the last value in the
sequence

```move
fun example(): u64 {
    let x = 0;
    x = x + 1;
    x // returns 'x'
}
```

See [the section below for more information on returns](#returning-values)

For more information on expression blocks, see [Move variables](./variables.md).

### Native Functions

Some functions do not have a body specified, and instead have the body provided by the VM. These
functions are marked `native`.

Without modifying the VM source code, a programmer cannot add new native functions. Furthermore, it
is the intent that `native` functions are used for either standard library code or for functionality
needed for the given Move environment.

Most `native` functions you will likely see are in standard library code, such as `vector`

```move
module std::vector {
    native public fun length<Element>(v: &vector<Element>): u64;
    ...
}
```

## Calling

When calling a function, the name can be specified either through an alias or fully qualified

```move
module a::example {
    public fun zero(): u64 { 0 }
}

module b::other {
    use a::example::{Self, zero};
    fun call_zero() {
        // With the `use` above all of these calls are equivalent
        a::example::zero();
        example::zero();
        zero();
    }
}
```

When calling a function, an argument must be given for every parameter.

```move
module a::example {
    public fun takes_none(): u64 { 0 }
    public fun takes_one(x: u64): u64 { x }
    public fun takes_two(x: u64, y: u64): u64 { x + y }
    public fun takes_three(x: u64, y: u64, z: u64): u64 { x + y + z }
}

module b::other {
    fun call_all() {
        a::example::takes_none();
        a::example::takes_one(0);
        a::example::takes_two(0, 1);
        a::example::takes_three(0, 1, 2);
    }
}
```

Type arguments can be either specified or inferred. Both calls are equivalent.

```move
module aexample {
    public fun id<T>(x: T): T { x }
}

module b::other {
    fun call_all() {
        a::example::id(0);
        a::example::id<u64>(0);
    }
}
```

For more details, see [Move generics](./generics.md).

## Returning values

The result of a function, its "return value", is the final value of its function body. For example

```move
fun add(x: u64, y: u64): u64 {
    x + y
}
```

The return value here is the result of `x + y`.

[As mentioned above](#function-body), the function's body is an [expression block](./variables.md).
The expression block can sequence various statements, and the final expression in the block will
be the value of that block

```move
fun double_and_add(x: u64, y: u64): u64 {
    let double_x = x * 2;
    let double_y = y * 2;
    double_x + double_y
}
```

The return value here is the result of `double_x + double_y`

### `return` expression

A function implicitly returns the value that its body evaluates to. However, functions can also use
the explicit `return` expression:

```move
fun f1(): u64 { return 0 }
fun f2(): u64 { 0 }
```

These two functions are equivalent. In this slightly more involved example, the function subtracts
two `u64` values, but returns early with `0` if the second value is too large:

```move
fun safe_sub(x: u64, y: u64): u64 {
    if (y > x) return 0;
    x - y
}
```

Note that the body of this function could also have been written as `if (y > x) 0 else x - y`.

However `return` really shines is in exiting deep within other control flow constructs. In this
example, the function iterates through a vector to find the index of a given value:

```move
use std::vector;
use std::option::{Self, Option};
fun index_of<T>(v: &vector<T>, target: &T): Option<u64> {
    let i = 0;
    let n = vector::length(v);
    while (i < n) {
        if (vector::borrow(v, i) == target) return option::some(i);
        i = i + 1
    };

    option::none()
}
```

Using `return` without an argument is shorthand for `return ()`. That is, the following two
functions are equivalent:

```move
fun foo() { return }
fun foo() { return () }
```
