# Methods

As a syntactic convience, some functions in Move can be called as "methods" on a value. This is done
by using the `.` operator to call the function, where the value on the left-hand side of the `.` is
the first argument to the function (sometimes called the receiver). The type of that value
statically determines which function is called. This is an important difference from some other
languages, where this syntax might indicate a dynamic call, where the function to be called is
determined at runtime. In Move, all function calls are statically determined.

In short, this syntax exists to make it easier to call functions without having to create an alias
with `use`, and without having to explicitly borrow the first argument to the function.
Additionally, this can make code more readable, as it reduces the amount of boilerplate needed to
call a function and makes it easier to chain function calls.

## Syntax

The syntax for calling a method is as follows:

```text
<expression> . <identifier> <[type_arguments],*> ( <arguments> )
```

For example

```move
coin.value();
*nums.borrow_mut(i) = 5;
```

## Method Resolution

When a method is called, the compiler will statically determine which function is called based on
the type of the receiver (the argument on the left-hand side of the `.`). The compiler maintains a
mapping from type and method name to the module and function name that should be called. This
mapping is created fom the `use fun` aliases that are currently in scope, and from the appropriate
functions in the receiver type's defining module. In all cases, the receiver type is the first
argument to the function, whether by-value or by-reference.

In this section, when we say a method "resolves" to a function, we mean that the compiler will
statically replace the method with a normal [function](./functions.md) call. For example if we have
`x.foo(e)` with `foo` resolving to `a::m::foo`, the compiler will replace `x.foo(e)` with
`a::m::foo(x, e)`, potentially [automatically borrowing](#automatic-borrowing) `x`.

### Functions in the Defining Module

In a type’s defining module, the compiler will automatically create a method alias for any function
declaration for its types when the type is the first argument in the function. For example,

```move
module a::m {
    public struct X() has copy, drop, store;
    public fun foo(x: &X) { ... }
    public fun bar(flag: bool, x: &X) { ... }
}
```

The function `foo` can be called as a method on a value of type `X`. However, not the first argument
(and one is not created for `bool` since `bool` is not defined in that module). For example,

```move
fun example(x: a::m::X) {
    x.foo(); // valid
    // x.bar(true); ERROR!
}
```

### `use fun` Aliases

Like a traditional [`use`](uses.md), a `use fun` statement creates an alias local to its current
scope. This could be for the current module or the current expression block. However, the alias is
associated to a type.

The syntax for a `use fun` statement is as follows:

```move
use fun <function> as <type>.<method alias>;
```

This creates an alias for the `<function>`, which the `<type>` can receive as `<method alias>`.

For example

```move
module a::cup {
    public struct Cup<T>(T) has copy, drop, store;

    public fun cup_borrow<T>(c: &Cup<T>): &T {
        &c.0
    }

    public fun cup_value<T>(c: Cup<T>): T {
        let Cup(t) = c;
        t
    }

    public fun cup_swap<T: drop>(c: &mut Cup<T>, t: T) {
        c.0 = t;
    }
}
```

We can now create `use fun` aliases to these functions

```move
module b::example {
    use fun a::cup::cup_borrow as Cup.borrow;
    use fun a::cup::cup_value as Cup.value;
    use fun a::cup::cup_swap as Cup.set;

    fun example(c: &mut Cup<u64>) {
        let _ = c.borrow(); // resolves to a::cup::cup_borrow
        let v = c.value(); // resolves to a::cup::cup_value
        c.set(v * 2); // resolves to a::cup::cup_swap
    }
}
```

Note that the `<function>` in the `use fun` does not have to be a fully resolved path, and an alias
can be used instead, so the declarations in the above example could equivalently be written as

```move
    use a::cup::{Self, cup_swap};

    use fun cup::cup_borrow as Cup.borrow;
    use fun cup::cup_value as Cup.value;
    use fun cup_swap as Cup.set;
```

While these examples are cute for renaming the functions in the current module, the feature is
perhaps more useful for declaring methods on types from other modules. For example, if we wanted to
add a new utility to `Cup`, we could do so with a `use fun` alias and still use method syntax

```move
module b::example {

    fun double(c: &Cup<u64>): Cup<u64> {
        let v = c.value();
        Cup::new(v * 2)
    }

}
```

Normally, we would be stuck having to call it as `double(&c)` because `b::example` did not define
`Cup`, but instead we can use a `use fun` alias

```move
    fun double_double(c: Cup<u64>): (Cup<u64>, Cup<u64>) {
        use fun b::example::double as Cup.dub;
        (c.dub(), c.dub()) // resolves to b::example::double in both calls
    }
```

While `use fun` can be made in any scope, the target `<function>` of the `use fun` must have a first
argument that is the same as the `<type>`.

```move
public struct X() has copy, drop, store;

fun new(): X { X() }
fun flag(flag: bool): u8 { if (flag) 1 else 0 }

use fun new as X.new; // ERROR!
use fun flag as X.flag; // ERROR!
// Neither `new` nor `flag` has first argument of type `X`
```

But any first argument of the `<type>` can be used, including references and mutable references

```move
public struct X() has copy, drop, store;

public fun by_val(_: X) {}
public fun by_ref(_: &X) {}
public fun by_mut(_: &mut X) {}

// All 3 valid, in any scope
use fun by_val as X.v;
use fun by_ref as X.r;
use fun by_mut as X.m;
```

Note for generics, the methods are associated for _all_ instances of the generic type. You cannot
overload the method to resolve to different functions depending on the instantiation.

```move
public struct Cup<T>(T) has copy, drop, store;

public fun value<T: copy>(c: &Cup<T>): T {
    c.0
}

use fun value as Cup<bool>.flag; // ERROR!
use fun value as Cup<u64>.num; // ERROR!
// In both cases, `use fun` aliases cannot be generic, they must work for all instances of the type
```

### `public use fun` Aliases

Unlike a traditional [`use`](uses.md), the `use fun` statement can be made `public`, which allows it
to be used outside of its declared scope. A `use fun` can be made `public` if it is declared in the
module that defines the receivers type, much like the method aliases that are
[automatically created](#functions-in-the-defining-module) for functions in the defining module. Or
conversely, one can think that an implicit `public use fun` is created automatically for every
function in the defining module that has a first argument of the receiver type (if it is defined in
that module). Both of these views are equivalent.

```move
module a::cup {
    public struct Cup<T>(T) has copy, drop, store;

    public use fun cup_borrow as Cup.borrow;
    public fun cup_borrow<T>(c: &Cup<T>): &T {
        &c.0
    }
}
```

In this example, a public method alias is created for `a::cup::Cup.borrow` and
`a::cup::Cup.cup_borrow`. Both resolve to `a::cup::cup_borrow`. And both are "public" in the sense
that they can be used outside of `a::cup`, without an additional `use` or `use fun`.

```move
module b::example {

    fun example<T: drop>(c: a::cup::Cup<u64>) {
        c.borrow(); // resolves to a::cup::cup_borrow
        c.cup_borrow(); // resolves to a::cup::cup_borrow
    }
}
```

The `public use fun` declarations thus serve as a way of renaming a function if you want to give it
a cleaner name for use with method syntax. This is especially helpful if you have a module with
multiple types, and similarly named functions for each type.

```move
module a::shapes {

    public struct Rectangle { base: u64, height: u64 }
    public struct Box { base: u64, height: u64, depth: u64 }

    // Rectangle and Box can have methods with the same name

    public use fun rectangle_base as Rectangle.base;
    public fun rectangle_base(rectangle: &Rectangle): u64 {
        rectangle.base
    }

    public use fun box_base as Box.base;
    public fun box_base(box: &Box): u64 {
        box.base
    }

}
```

Another use for `public use fun` is adding methods to types from other modules. This can be helpful
in conjunction with functions spread out across a single package.

```move
module a::cup {
    public struct Cup<T>(T) has copy, drop, store;

    public fun new<T>(t: T): Cup<T> { Cup(t) }
    public fun borrow<T>(c: &Cup<T>): &T {
        &c.0
    }
    // `public use fun` to a function defined in another module
    public use fun a::utils::split as Cup.split;
}

module a::utils {
    use a::m::{Self, Cup};

    public fun split<u64>(c: Cup<u64>): (Cup<u64>, Cup<u64>) {
        let Cup(t) = c;
        let half = t / 2;
        let rem = if (t > 0) t - half else 0;
        (cup::new(half), cup::new(rem))
    }

}
```

And note that this `public use fun` does not create a circular dependency, as the `use fun` is not
present after the module is compiled--all methods are resolved statically.

### Interactions with `use` Aliases

A small detail to note is that method aliases respect normal `use` aliases.

```move
module a::cup {
    public struct Cup<T>(T) has copy, drop, store;

    public fun cup_borrow<T>(c: &Cup<T>): &T {
        &c.0
    }
}

module b::other {
    use a::cup::{Cup, cup_borrow as borrow};

    fun example(c: &Cup<u64>) {
        c.borrow(); // resolves to a::cup::cup_borrow
    }
}
```

A helpful way to think about this is that `use` creates an implicit `use fun` alias for the function
whenever it can. In this case the `use a::cup::cup_borrow as borrow` creates an implicit
`use fun a::cup::cup_borrow as Cup.borrow` because it would be a valid `use fun` alias. Both views
are equivalent. This line of reasoning can inform how specific methods will resolve with shadowing.
See the cases in [Scoping](#scoping) for more details.

### Scoping

If not `public`, a `use fun` alias is local to its scope, much like a normal [`use`](uses.md). For
example

```move
module a::m {
    public struct X() has copy, drop, store;
    public fun foo(_: &X) {}
    public fun bar(_: &X) {}
}

module b::other {
    use a::m::X;

    use fun a::m::foo as X.f;

    fun example(x: &X) {
        x.f(); // resolves to a::m::foo
        {
            use a::m::bar as f;
            x.f(); // resolves to a::m::bar
        };
        x.f(); // still resolves to a::m::foo
        {
            use fun a::m::bar as X.f;
            x.f(); // resolves to a::m::bar
        }
    }
```

## Automatic Borrowing

When resolving a method, the compiler will automatically borrow the receiver if the function expects
a reference. For example

```move
module a::m {
    public struct X() has copy, drop;
    public fun by_val(_: X) {}
    public fun by_ref(_: &X) {}
    public fun by_mut(_: &mut X) {}

    fun example(mut x: X) {
        x.by_ref(); // resolves to a::m::by_ref(&x)
        x.by_mut(); // resolves to a::m::by_mut(&mut x)
    }
}
```

In these examples, `x` was automatically borrowed to `&x` and `&mut x` respectively. This will also
work through field access

```move
module a::m {
    public struct X() has copy, drop;
    public fun by_val(_: X) {}
    public fun by_ref(_: &X) {}
    public fun by_mut(_: &mut X) {}

    public struct Y has drop { x: X }

    fun example(mut y: Y) {
        y.x.by_ref(); // resolves to a::m::by_ref(&y.x)
        y.x.by_mut(); // resolves to a::m::by_mut(&mut y.x)
    }
}
```

Note that in both examples, the local variable had to be labeled as [`mut`](./variables.md) to allow
for the `&mut` borrow. Without this, there would be an error saying that `x` (or `y` in the second
example) is not mutable.

Keep in mind that without a reference, normal rules for variable and field access come into play.
Meaning a value might be moved or copied if it is not borrowed.

```move
module a::m {
    public struct X() has copy, drop;
    public fun by_val(_: X) {}
    public fun by_ref(_: &X) {}
    public fun by_mut(_: &mut X) {}

    public struct Y has drop { x: X }
    public fun drop_y(y: Y) { y }

    fun example(y: Y) {
        y.x.by_val(); // copies `y.x` since `by_val` is by-value and `X` has `copy`
        y.drop_y(); // moves `y` since `drop_y` is by-value and `Y` does _not_ have `copy`
    }
}
```

## Chaining

Method calls can be chained, because any expression can be the receiver of the method.

```move
module a::shapes {
    public struct Point has copy, drop, store { x: u64, y: u64 }
    public struct Line has copy, drop, store { start: Point, end: Point }

    public fun x(p: &Point): u64 { p.x }
    public fun y(p: &Point): u64 { p.y }

    public fun start(l: &Line): &Point { &l.start }
    public fun end(l: &Line): &Point { &l.end }

}

module b::example {
    use a::shapes::Line;

    public fun x_values(l: Line): (u64, u64) {
        (l.start().x(), l.end().x())
    }

}
```

In this example for `l.start().x()`, the compiler first resolves `l.start()` to
`a::shapes::start(&l)`. Then `.x()` is resolved to `a::shapes::x(a::shapes::start(&l))`. Similarly
for `l.end().x()`. Keep in mind, this feature is not "special"--the left-hand side of the `.` can be
any expression, and the compiler will resolve the method call as normal. We simply draw attention to
this sort of "chaining" because it is a common practice to increase readability.
