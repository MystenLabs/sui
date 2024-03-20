# Uses and Aliases

The `use` syntax can be used to create aliases to members in other modules. `use` can be used to
create aliases that last either for the entire module, or for a given expression block scope.

## Syntax

There are several different syntax cases for `use`. Starting with the most simple, we have the
following for creating aliases to other modules

```move
use <address>::<module name>;
use <address>::<module name> as <module alias name>;
```

For example

```move
use std::vector;
use std::vector as V;
```

`use std::vector;` introduces an alias `vector` for `std::vector`. This means that anywhere you
would want to use the module name `std::vector` (assuming this `use` is in scope), you could use
`vector` instead. `use std::vector;` is equivalent to `use std::vector as vector;`

Similarly `use std::vector as V;` would let you use `V` instead of `std::vector`

```move=
use std::vector;
use std::vector as V;

fun new_vecs(): (vector<u8>, vector<u8>, vector<u8>) {
    let v1 = std::vector::empty();
    let v2 = vector::empty();
    let v3 = V::empty();
    (v1, v2, v3)
}
```

If you want to import a specific module member (such as a function, struct, or constant). You can
use the following syntax.

```move
use <address>::<module name>::<module member>;
use <address>::<module name>::<module member> as <member alias>;
```

For example

```move
use std::vector::empty;
use std::vector::empty as empty_vec;
```

This would let you use the function `std::vector::empty` without full qualification. Instead you
could use `empty` and `empty_vec` respectively. Again, `use std::vector::empty;` is equivalent to
`use std::vector::empty as empty;`

```move=
use std::vector::empty;
use std::vector::empty as empty_vec;

fun new_vecs(): (vector<u8>, vector<u8>, vector<u8>) {
    let v1 = std::vector::empty();
    let v2 = empty();
    let v3 = empty_vec();
    (v1, v2, v3)
}
```

If you want to add aliases for multiple module members at once, you can do so with the following
syntax

```move
use <address>::<module name>::{<module member>, <module member> as <member alias> ... };
```

For example

```move=
use std::vector::{push_back, length as len, pop_back};

fun swap_last_two<T>(v: &mut vector<T>) {
    assert!(len(v) >= 2, 42);
    let last = pop_back(v);
    let second_to_last = pop_back(v);
    push_back(v, last);
    push_back(v, second_to_last)
}
```

If you need to add an alias to the Module itself in addition to module members, you can do that in a
single `use` using `Self`. `Self` is a member of sorts that refers to the module.

```move
use std::vector::{Self, empty};
```

For clarity, all of the following are equivalent:

```move
use std::vector;
use std::vector as vector;
use std::vector::Self;
use std::vector::Self as vector;
use std::vector::{Self};
use std::vector::{Self as vector};
```

If needed, you can have as many aliases for any item as you like

```move=
use std::vector::{
    Self,
    Self as V,
    length,
    length as len,
};

fun pop_twice<T>(v: &mut vector<T>): (T, T) {
    // all options available given the `use` above
    assert!(vector::length(v) > 1, 42);
    assert!(V::length(v) > 1, 42);
    assert!(length(v) > 1, 42);
    assert!(len(v) > 1, 42);

    (vector::pop_back(v), vector::pop_back(v))
}
```

## Inside a `module`

Inside of a `module` all `use` declarations are usable regardless of the order of declaration.

```move=
address 0x42 {
module example {
    use std::vector;

    fun example(): vector<u8> {
        let v = empty();
        vector::push_back(&mut v, 0);
        vector::push_back(&mut v, 10);
        v
    }

    use std::vector::empty;
}
}
```

The aliases declared by `use` in the module usable within that module.

Additionally, the aliases introduced cannot conflict with other module members. See
[Uniqueness](#uniqueness) for more details

## Inside an expression

You can add `use` declarations to the beginning of any expression block

```move=
address 0x42 {
module example {

    fun example(): vector<u8> {
        use std::vector::{empty, push_back};

        let v = empty();
        push_back(&mut v, 0);
        push_back(&mut v, 10);
        v
    }
}
}
```

As with `let`, the aliases introduced by `use` in an expression block are removed at the end of that
block.

```move=
address 0x42 {
module example {

    fun example(): vector<u8> {
        let result = {
            use std::vector::{empty, push_back};
            let v = empty();
            push_back(&mut v, 0);
            push_back(&mut v, 10);
            v
        };
        result
    }

}
}
```

Attempting to use the alias after the block ends will result in an error

```move=
fun example(): vector<u8> {
    let result = {
        use std::vector::{empty, push_back};
        let v = empty();
        push_back(&mut v, 0);
        push_back(&mut v, 10);
        v
    };
    let v2 = empty(); // ERROR!
//           ^^^^^ unbound function 'empty'
    result
}
```

Any `use` must be the first item in the block. If the `use` comes after any expression or `let`, it
will result in a parsing error

```move=
{
    let x = 0;
    use std::vector; // ERROR!
    let v = vector::empty();
}
```

## Naming rules

Aliases must follow the same rules as other module members. This means that aliases to structs or
constants must start with `A` to `Z`

```move=
address 0x42 {
module data {
    struct S {}
    const FLAG: bool = false;
    fun foo() {}
}
module example {
    use 0x42::data::{
        S as s, // ERROR!
        FLAG as fLAG, // ERROR!
        foo as FOO,  // valid
        foo as bar, // valid
    };
}
}
```

## Uniqueness

Inside a given scope, all aliases introduced by `use` declarations must be unique.

For a module, this means aliases introduced by `use` cannot overlap

```move=
address 0x42 {
module example {

    use std::vector::{empty as foo, length as foo}; // ERROR!
    //                                        ^^^ duplicate 'foo'

    use std::vector::empty as bar;

    use std::vector::length as bar; // ERROR!
    //                         ^^^ duplicate 'bar'

}
}
```

And, they cannot overlap with any of the module's other members

```move=
address 0x42 {
module data {
    struct S {}
}
module example {
    use 0x42::data::S;

    struct S { value: u64 } // ERROR!
    //     ^ conflicts with alias 'S' above
}
}
```

Inside of an expression block, they cannot overlap with each other, but they can
[shadow](#shadowing) other aliases or names from an outer scope

## Shadowing

`use` aliases inside of an expression block can shadow names (module members or aliases) from the
outer scope. As with shadowing of locals, the shadowing ends at the end of the expression block;

```move=
address 0x42 {
module example {

    struct WrappedVector { vec: vector<u64> }

    fun empty(): WrappedVector {
        WrappedVector { vec: std::vector::empty() }
    }

    fun example1(): (WrappedVector, WrappedVector) {
        let vec = {
            use std::vector::{empty, push_back};
            // 'empty' now refers to std::vector::empty

            let v = empty();
            push_back(&mut v, 0);
            push_back(&mut v, 1);
            push_back(&mut v, 10);
            v
        };
        // 'empty' now refers to Self::empty

        (empty(), WrappedVector { vec })
    }

    fun example2(): (WrappedVector, WrappedVector) {
        use std::vector::{empty, push_back};
        let w: WrappedVector = {
            use 0x42::example::empty;
            empty()
        };
        push_back(&mut w.vec, 0);
        push_back(&mut w.vec, 1);
        push_back(&mut w.vec, 10);

        let vec = empty();
        push_back(&mut vec, 0);
        push_back(&mut vec, 1);
        push_back(&mut vec, 10);

        (w, WrappedVector { vec })
    }
}
}
```

## Unused Use or Alias

An unused `use` will result in an error

```move=
address 0x42 {
module example {
    use std::vector::{empty, push_back}; // ERROR!
    //                       ^^^^^^^^^ unused alias 'push_back'

    fun example(): vector<u8> {
        empty()
    }
}
}
```
