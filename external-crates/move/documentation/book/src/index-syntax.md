# Index Syntax

Syntax attributes establish a way to allow programmers to define operations for syntax-like usage in
Move. Our first syntax method, `index`, allows programmers to define a group of operations that
can be used as custom index accessors for their datatypes. These definitions allow programmers to
use index syntax, such as accessing a matrix element as `m[i,j]`, all based on user definitions.
These definitions are bespoke per-type and available implicitly for any programmer using the type,
easing usage across the language.

## Overview and Summary

Consider a `Matrix` type that uses a vector of vectors to represent its values. We can write a small
library using `index` syntax annotations on its `borrow` functions as follows:

```
module matrix {

    public struct Matrix<T> { v: vector<vector<T>> }

    #[syntax(index)]
    public fun borrow<T>(s: &Matrix<T>, i: u64, j: u64):  &T {
        borrow(borrow(s.v, i), j)
    }

    #[syntax(index)]
    public fun borrow_mut<T>(s: &mut Matrix<T>, i: u64, j: u64):  &mut T {
        borrow_mut(borrow_mut(s.v, i), j)
    }

    public fun make_matrix<T>(v: vector<vector<T>>):  Matrix<T> {
        Matrix { v }
    }

}
```

Now anyone using this Matrix type has access to index syntax for it:

```
let v0 = vector<u64>[1, 0, 0];
let v1 = vector<u64>[0, 1, 0];
let v2 = vector<u64>[0, 0, 1];
let v = vector<vector<u64>>[v0, v1, v2];
let mut m = matrix::make_matrix(v);

let mut i = 0;
while (i < 3) {
    let mut j = 0;
    while (j < 3) {
        if (i == j) {
            assert!(m[i, j] == 1, i);
        } else {
            assert!(m[i, j] == 0, i + 10);
        };
        *(&mut m[i,j]) = 2;
        j = j + 1;
    };
    i = i + 1;
}
```

## Usage

As the example indicates, if a datatype defines an index syntax function, it may be used by
invoking index syntax on a value of that type:

```move
let mat = matrix::make_matrix(...);
let m_0_0 = mat[0, 0];
```

During compilation, the compiler will translate these into the appropriate function invocations
based on the position and mutable usage of the expression:

```move
let mut mat = matrix::make_matrix(...);

let m_0_0 = mat[0, 0];
    // translates to copy matrix::borrow(&mat, 0, 0)

let m_0_0 = &mat[0, 0];
    // translates to matrix::borrow(&mat, 0, 0)

let m_0_0 = &mut mat[0, 0];
    // translates to matrix::borrow_mut(&mut mat, 0, 0)
``

Index expressions may also be intermixed with field accesses:

```move
public struct V { v: vector<u64> }

public struct Vs { vs: vector<V> }

fun borrow_first(input: &Vs): &u64 {
    input.vs[0].v[0]
    // translates to vector::borrow(vector::borrow(input.vs, 0).v, 0)
}
```

### Index functions take flexible arguments

Note that, aside from the definition and type limitations described in the rest of this chapter,
there are no restrictions on the values that may be passed as index parameters, allowing for
intricate programmatic behavior when using index syntax. For example, a data structure might wish to
take a default value if the index is out of bounds:

```
#[syntax(index)]
public fun borrow_or_set<Key: copy, Value: drop>(
    input: &mut MTable<Key, Value>,
    key: &Key,
    default: Value
): &mut Value {
    if (contains(input, *key)) {
        borrow(input, key)
    } else {
        insert(input, *key, default)
        borrow(input, key)
    }
}
```

Indexing into `MTable` would then require the user provide a default value:

```
let string_key: String = ...;
let mut table: MTable<String, u64> = m_table::make_table();
let entry: &mut u64 = &mut table[string_key, 0];
```

This sort of extensible power allows developers to write precise index interfaces for their types,
concretely enforcing bespoke behavior.


## Defining Index Syntax Functions

This powerful syntax form allows for all user-defined datatypes to behave in this way, assuming they
adhere to the following rules:

1. The `#[syntax(index)]` attribute is added to the designated functions defined in the same module
   as the subject type.
2. The designated functions have `public` visibility.
3. The functions take a reference type as its "subject" type (its first argument) and returns a
   matching references type (`mut` if the subject was `mut`).
4. Each type has only a single mutable and single immutable definition.
5. Immutable and mutable versions have type agreement:
    - The subject types match, differing only in mutability.
    - The return types match the mutability of their subject types.
    - Type parameters, if present, have identical constraints between both versions.
    - All parameters beyond the subject type are identical.

These are described in greater detail, with additional examples, below.

### Declaration

To declare an index syntax method, you add the `#[syntax(index)]` attribute above the relevant
function definition in the same module as the subject type's definition. This signals to the
compiler that the function is an index accessor for the specified type.

#### Immutable Accessor

The immutable index syntax method is defined for read-only accesses. It takes an immutable reference
of the subject type and returns an immutable reference to the element type. The `borrow` function
defined in `std::vector` is an example of this:

```move
#[syntax(index)]
public fun borrow<Element>(v: &vector<Element>, i: u64): &Element {
    // implementation
}
```

#### Mutable Accessor

The mutable index syntax method is the dual of the immutable one, allowing for both read and write
operations. It takes a mutable reference of the subject type and returns an mutable reference to
the element type. The `borrow_mut` function defined in `std::vector` is an example of this:

```move
#[syntax(index)]
public fun borrow_mut<Element>(v: &mut vector<Element>, i: u64): &mut Element {
    // implementation
}
```

#### Visibility

To ensure that indexing functions are available anywhere the type is used, all index syntax methods
must have public visibility. This ensures ergonomic usage of indexing across modules and packages in
Move.

#### No Duplicates

In addition to the above requirements, we restrict each subject base type to defining a single
index syntax method for immutable references and a single index syntax method for mutable
references. For example, you may not define a specialized version for a polymorphic type:

```move
#[syntax(index)]
public fun borrow_matrix_u64(s: &Matrix<u64>, i: u64, j: u64): &u64 { ... }

#[syntax(index)]
public fun borrow_matrix<T>(s: &Matrix<T>, i: u64, j: u64): &T { ... }
    // ERROR! Matrix already has a definition
    // for its immutable index syntax method
```

This ensures that the user can always tell which method is being invoked, without the need to
inspect type instantiation.

### Type Constraints

By default, an index syntax method has the following type constraints:

**Its subject type (first argument) must be a reference to a single type defined in the same module
as the marked function.** This means that we cannot define index syntax methods for tuples,
type parameters, or values:

```move
#[syntax(index)]
public fun borrow_fst(x: &(u64, u64), ...): &u64 { ... }
    // ERROR because the subject type is a tuple

#[syntax(index)]
public fun borrow_tyarg<T>(x: &T, ...): &T { ... }
    // ERROR because the subject type is a type parameter

#[syntax(index)]
public fun borrow_value(x: Matrix<u64>, ...): &u64 { ... }
    // ERROR because x is not a reference
```

**The subject type must match mutability with the return type.** This restriction allows users to
clarify the expected behavior when they borrow an indexed expression as `&vec[i]` versus `&mut
vec[i]`. The Move compiler uses the mutability marker to determine which borrow form to call to
produce a reference of the appropriate mutability. As a result, we disallow index syntax methods
whose subject and return mutability differ:

```move
#[syntax(index)]
public fun borrow_imm(x: &mut Matrix<u64>, ...): &u64 { ... }
    // ERROR! incompatible mutability

```

### Type Compatibility

When defining an immutable and mutable index syntax method pair, they are subject to a number of
compatibility constraints:

1. They must take the same number of type parameters, those type parameters must have the same
   constraints.
5. Type parameters must be used the same _by position_, not name.
2. Their subject types must match exactly except for the mutability.
3. Their return types must match exactly except for the mutability.
4. All other parameter types must match exactly.
These constraints are to ensure that index syntax behaves identically
regardless of being in a mutable or immutable position.

To illustrate some of these errors, recall the Matrix definition above:

```move
#[syntax(index)]
public fun borrow<T>(s: &Matrix<T>, i: u64, j: u64):  &T {
    borrow(borrow(s.v, i), j)
}
```

All of the following are type-incompatible definitions of the mutable version:

```move
#[syntax(index)]
public fun borrow_mut<T: drop>(s: &mut Matrix<T>, i: u64, j: u64):  &mut T { ... }
    // ERROR! `T` has `drop` here, but no in the immutable version

#[syntax(index)]
public fun borrow_mut(s: &mut Matrix<u64>, i: u64, j: u64):  &mut u64 { ... }
    // ERROR! This takes a different number of type parameters

#[syntax(index)]
public fun borrow_mut<T, U>(s: &mut Matrix<U>, i: u64, j: u64):  &mut U { ... }
    // ERROR! This takes a different number of type parameters

#[syntax(index)]
public fun borrow_mut<T, U>(s: &mut Matrix<U>, i_j: (u64, u64)):  &mut U { ... }
    // ERROR! This takes a different number of arguments

#[syntax(index)]
public fun borrow_mut<T, U>(s: &mut Matrix<U>, i: u64, j: u32):  &mut U { ... }
    // ERROR! `j` is a different type
```

Again, the gaol here is to make the usage across the immutable and mutable versions consistent. This
allows index syntax methods to work without changing the behavior or constraints out based on
mutable versus immutable usage, ultimately ensuring a consistent interface to program against.

