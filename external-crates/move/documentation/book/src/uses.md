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
use std::option as o;
```

`use std::vector;` introduces an alias `vector` for `std::vector`. This means that anywhere you
would want to use the module name `std::vector` (assuming this `use` is in scope), you could use
`vector` instead. `use std::vector;` is equivalent to `use std::vector as vector;`

Similarly `use std::option as o;` would let you use `o` instead of `std::option`

```move
use std::vector;
use std::option as o;

fun new_vec(): vector<o::Option<u8>> {
    let mut v = vector[];
    vector::push_back(&mut v, o::some(0));
    vector::push_back(&mut v, o::none());
    v
}
```

If you want to import a specific module member (such as a function or struct). You can use the
following syntax.

```move
use <address>::<module name>::<module member>;
use <address>::<module name>::<module member> as <member alias>;
```

For example

```move
use std::vector::push_back;
use std::option::some as s;
```

This would let you use the function `std::vector::push_back` without full qualification. Similarly
for `std::option::some` with `s`. Instead you could use `push_back` and `s` respectively. Again,
`use std::vector::push_back;` is equivalent to `use std::vector::push_back as push_back;`

```move
use std::vector::push_back;
use std::option::some as s;

fun new_vec(): vector<std::option::Option<u8>> {
    let mut v = vector[];
    vector::push_back(&mut v, s(0));
    vector::push_back(&mut v, std::option::none());
    v
}
```

### Multiple Aliases

If you want to add aliases for multiple module members at once, you can do so with the following
syntax

```move
use <address>::<module name>::{<module member>, <module member> as <member alias> ... };
```

For example

```move
use std::vector::push_back;
use std::option::{some as s, none as n};

fun new_vec(): vector<std::option::Option<u8>> {
    let mut v = vector[];
    push_back(&mut v, s(0));
    push_back(&mut v, n());
    v
}
```

### Self aliases

If you need to add an alias to the Module itself in addition to module members, you can do that in a
single `use` using `Self`. `Self` is a member of sorts that refers to the module.

```move
use std::option::{Self, some, none};
```

For clarity, all of the following are equivalent:

```move
use std::option;
use std::option as option;
use std::option::Self;
use std::option::Self as option;
use std::option::{Self};
use std::option::{Self as option};
```

### Multiple Aliases for the Same Definition

If needed, you can have as many aliases for any item as you like

```move
use std::vector::push_back;
use std::option::{Option, some, none};

fun new_vec(): vector<Option<u8>> {
    let mut v = vector[];
    push_back(&mut v, some(0));
    push_back(&mut v, none());
    v
}
```

### Nested imports

In Move, you can also import multiple names with the same `use` declaration. This brings all
provided names into scope:

```move
use std::{
    vector::{Self as vec, push_back},
    string::{String, Self as str}
};

fun example(s: &mut String) {
    let mut v = vec::empty();
    push_back(&mut v, 0);
    push_back(&mut v, 10);
    str::append_utf8(s, v);
}
```

## Inside a `module`

Inside of a `module` all `use` declarations are usable regardless of the order of declaration.

```move
module a::example {
    use std::vector;

    fun new_vec(): vector<Option<u8>> {
        let mut v = vector[];
        vector::push_back(&mut v, 0);
        vector::push_back(&mut v, 10);
        v
    }

    use std::option::{Option, some, none};
}
```

The aliases declared by `use` in the module usable within that module.

Additionally, the aliases introduced cannot conflict with other module members. See
[Uniqueness](#uniqueness) for more details

## Inside an expression

You can add `use` declarations to the beginning of any expression block

```move
module a::example {
    fun new_vec(): vector<Option<u8>> {
        use std::vector::push_back;
        use std::option::{Option, some, none};

        let mut v = vector[];
        push_back(&mut v, some(0));
        push_back(&mut v, none());
        v
    }
}
```

As with `let`, the aliases introduced by `use` in an expression block are removed at the end of that
block.

```move
module a::example {
    fun new_vec(): vector<Option<u8>> {
        let result = {
            use std::vector::push_back;
            use std::option::{Option, some, none};

            let mut v = vector[];
            push_back(&mut v, some(0));
            push_back(&mut v, none());
            v
        };
        result
    }
}
```

Attempting to use the alias after the block ends will result in an error

```move
    fun new_vec(): vector<Option<u8>> {
        let mut result = {
            use std::vector::push_back;
            use std::option::{Option, some, none};

            let mut v = vector[];
            push_back(&mut v, some(0));
            v
        };
        push_back(&mut result, std::option::none());
        // ^^^^^^ ERROR! unbound function 'push_back'
        result
    }
```

Any `use` must be the first item in the block. If the `use` comes after any expression or `let`, it
will result in a parsing error

```move
{
    let mut v = vector[];
    use std::vector; // ERROR!
}
```

This allows you to shorten your import blocks in many situations. Note that these imports, as the
previous ones, are all subject to the naming and uniqueness rules described in the following
sections.

## Naming rules

Aliases must follow the same rules as other module members. This means that aliases to structs (and
constants) must start with `A` to `Z`

```move
module a::data {
    public struct S {}
    const FLAG: bool = false;
    public fun foo() {}
}
module a::example {
    use a::data::{
        S as s, // ERROR!
        FLAG as fLAG, // ERROR!
        foo as FOO,  // valid
        foo as bar, // valid
    };
}
```

## Uniqueness

Inside a given scope, all aliases introduced by `use` declarations must be unique.

For a module, this means aliases introduced by `use` cannot overlap

```move
module a::example {
    use std::option::{none as foo, some as foo}; // ERROR!
    //                                     ^^^ duplicate 'foo'

    use std::option::none as bar;

    use std::option::some as bar; // ERROR!
    //                       ^^^ duplicate 'bar'

}
```

And, they cannot overlap with any of the module's other members

```move
module a::data {
    public struct S {}
}
module example {
    use a::data::S;

    public struct S { value: u64 } // ERROR!
    //            ^ conflicts with alias 'S' above
}
}
```

Inside of an expression block, they cannot overlap with each other, but they can
[shadow](#shadowing) other aliases or names from an outer scope

## Shadowing

`use` aliases inside of an expression block can shadow names (module members or aliases) from the
outer scope. As with shadowing of locals, the shadowing ends at the end of the expression block;

```move
module a::example {

    public struct WrappedVector { vec: vector<u64> }

    public fun empty(): WrappedVector {
        WrappedVector { vec: std::vector::empty() }
    }

    public fun push_back(v: &mut WrappedVector, value: u64) {
        std::vector::push_back(&mut v.vec, value);
    }

    fun example1(): WrappedVector {
        use std::vector::push_back;
        // 'push_back' now refers to std::vector::push_back
        let mut vec = vector[];
        push_back(&mut vec, 0);
        push_back(&mut vec, 1);
        push_back(&mut vec, 10);
        WrappedVector { vec }
    }

    fun example2(): WrappedVector {
        let vec = {
            use std::vector::push_back;
            // 'push_back' now refers to std::vector::push_back

            let mut v = vector[];
            push_back(&mut v, 0);
            push_back(&mut v, 1);
            v
        };
        // 'push_back' now refers to Self::push_back
        let mut res = WrappedVector { vec };
        push_back(&mut res, 10);
        res
    }
}
```

## Unused Use or Alias

An unused `use` will result in a warning

```move
module a::example {
    use std::option::{some, none}; // Warning!
    //                      ^^^^ unused alias 'none'

    public fun example(): std::option::Option<u8> {
        some(0)
    }
}
```
