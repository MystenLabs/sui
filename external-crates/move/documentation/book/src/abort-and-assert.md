# Abort and Assert

[`return`](./functions.md) and `abort` are two control flow constructs that end execution, one for
the current function and one for the entire transaction.

More information on [`return` can be found in the linked section](./functions.md)

## `abort`

`abort` is an expression that takes one argument: an **abort code** of type `u64`. For example:

```move
abort 42
```

The `abort` expression halts execution the current function and reverts all changes made to global
state by the current transaction. There is no mechanism for "catching" or otherwise handling an
`abort`.

Luckily, in Move transactions are all or nothing, meaning any changes to global storage are made all
at once only if the transaction succeeds. Because of this transactional commitment of changes, after
an abort there is no need to worry about backing out changes. While this approach is lacking in
flexibility, it is incredibly simple and predictable.

Similar to [`return`](./functions.md), `abort` is useful for exiting control flow when some
condition cannot be met.

In this example, the function will pop two items off of the vector, but will abort early if the
vector does not have two items

```move=
use std::vector;
fun pop_twice<T>(v: &mut vector<T>): (T, T) {
    if (vector::length(v) < 2) abort 42;

    (vector::pop_back(v), vector::pop_back(v))
}
```

This is even more useful deep inside a control-flow construct. For example, this function checks
that all numbers in the vector are less than the specified `bound`. And aborts otherwise

```move=
use std::vector;
fun check_vec(v: &vector<u64>, bound: u64) {
    let i = 0;
    let n = vector::length(v);
    while (i < n) {
        let cur = *vector::borrow(v, i);
        if (cur > bound) abort 42;
        i = i + 1;
    }
}
```

### `assert`

`assert` is a builtin, macro-like operation provided by the Move compiler. It takes two arguments, a
condition of type `bool` and a code of type `u64`

```move
assert!(condition: bool, code: u64)
```

Since the operation is a macro, it must be invoked with the `!`. This is to convey that the
arguments to `assert` are call-by-expression. In other words, `assert` is not a normal function and
does not exist at the bytecode level. It is replaced inside the compiler with

```move
if (condition) () else abort code
```

`assert` is more commonly used than just `abort` by itself. The `abort` examples above can be
rewritten using `assert`

```move=
use std::vector;
fun pop_twice<T>(v: &mut vector<T>): (T, T) {
    assert!(vector::length(v) >= 2, 42); // Now uses 'assert'

    (vector::pop_back(v), vector::pop_back(v))
}
```

and

```move=
use std::vector;
fun check_vec(v: &vector<u64>, bound: u64) {
    let i = 0;
    let n = vector::length(v);
    while (i < n) {
        let cur = *vector::borrow(v, i);
        assert!(cur <= bound, 42); // Now uses 'assert'
        i = i + 1;
    }
}
```

Note that because the operation is replaced with this `if-else`, the argument for the `code` is not
always evaluated. For example:

```move
assert!(true, 1 / 0)
```

Will not result in an arithmetic error, it is equivalent to

```move
if (true) () else (1 / 0)
```

So the arithmetic expression is never evaluated!

### Abort codes in the Move VM

When using `abort`, it is important to understand how the `u64` code will be used by the VM.

Normally, after successful execution, the Move VM produces a change-set for the changes made to
global storage (added/removed resources, updates to existing resources, etc).

If an `abort` is reached, the VM will instead indicate an error. Included in that error will be two
pieces of information:

- The module that produced the abort (address and name)
- The abort code.

For example

```move=
address 0x2 {
module example {
    public fun aborts() {
        abort 42
    }
}
}

script {
    fun always_aborts() {
        0x2::example::aborts()
    }
}
```

If a transaction, such as the script `always_aborts` above, calls `0x2::example::aborts`, the VM
would produce an error that indicated the module `0x2::example` and the code `42`.

This can be useful for having multiple aborts being grouped together inside a module.

In this example, the module has two separate error codes used in multiple functions

```move=
address 0x42 {
module example {

    use std::vector;

    const EMPTY_VECTOR: u64 = 0;
    const INDEX_OUT_OF_BOUNDS: u64 = 1;

    // move i to j, move j to k, move k to i
    public fun rotate_three<T>(v: &mut vector<T>, i: u64, j: u64, k: u64) {
        let n = vector::length(v);
        assert!(n > 0, EMPTY_VECTOR);
        assert!(i < n, INDEX_OUT_OF_BOUNDS);
        assert!(j < n, INDEX_OUT_OF_BOUNDS);
        assert!(k < n, INDEX_OUT_OF_BOUNDS);

        vector::swap(v, i, k);
        vector::swap(v, j, k);
    }

    public fun remove_twice<T>(v: &mut vector<T>, i: u64, j: u64): (T, T) {
        let n = vector::length(v);
        assert!(n > 0, EMPTY_VECTOR);
        assert!(i < n, INDEX_OUT_OF_BOUNDS);
        assert!(j < n, INDEX_OUT_OF_BOUNDS);
        assert!(i > j, INDEX_OUT_OF_BOUNDS);

        (vector::remove<T>(v, i), vector::remove<T>(v, j))
    }
}
}
```

## The type of `abort`

The `abort i` expression can have any type! This is because both constructs break from the normal
control flow, so they never need to evaluate to the value of that type.

The following are not useful, but they will type check

```move
let y: address = abort 0;
```

This behavior can be helpful in situations where you have a branching instruction that produces a
value on some branches, but not all. For example:

```move
let b =
    if (x == 0) false
    else if (x == 1) true
    else abort 42;
//       ^^^^^^^^ `abort 42` has type `bool`
```
