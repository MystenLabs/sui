# Local Variables and Scope

Local variables in Move are lexically (statically) scoped. New variables are introduced with the
keyword `let`, which will shadow any previous local with the same name. Locals marked as `mut` are
mutable and can be updated both directly and via a mutable reference.

## Declaring Local Variables

### `let` bindings

Move programs use `let` to bind variable names to values:

```move
let x = 1;
let y = x + x:
```

`let` can also be used without binding a value to the local.

```move
let x;
```

The local can then be assigned a value later.

```move
let x;
if (cond) {
  x = 1
} else {
  x = 0
}
```

This can be very helpful when trying to extract a value from a loop when a default value cannot be
provided.

```move
let x;
let cond = true;
let i = 0;
loop {
    let (res, cond) = foo(i);
    if (!cond) {
        x = res;
        break;
    };
    i = i + 1;
}
```

To modify a local variable _after_ it is assigned, or to borrow it mutably `&mut`, it must be
declared as `mut`.

```move
let mut x = 0;
if (cond) x = x + 1;
foo(&mut x);
```

For more details see the section on [assignments](#assignments) below.

### Variables must be assigned before use

Move's type system prevents a local variable from being used before it has been assigned.

```move
let x;
x + x // ERROR! x is used before being assigned
```

```move
let x;
if (cond) x = 0;
x + x // ERROR! x does not have a value in all cases
```

```move
let x;
while (cond) x = 0;
x + x // ERROR! x does not have a value in all cases
```

### Valid variable names

Variable names can contain underscores `_`, letters `a` to `z`, letters `A` to `Z`, and digits `0`
to `9`. Variable names must start with either an underscore `_` or a letter `a` through `z`. They
_cannot_ start with uppercase letters.

```move
// all valid
let x = e;
let _x = e;
let _A = e;
let x0 = e;
let xA = e;
let foobar_123 = e;

// all invalid
let X = e; // ERROR!
let Foo = e; // ERROR!
```

### Type annotations

The type of a local variable can almost always be inferred by Move's type system. However, Move
allows explicit type annotations that can be useful for readability, clarity, or debuggability. The
syntax for adding a type annotation is:

```move
let x: T = e; // "Variable x of type T is initialized to expression e"
```

Some examples of explicit type annotations:

```move
module 0x42::example {

    public struct S { f: u64, g: u64 }

    fun annotated() {
        let u: u8 = 0;
        let b: vector<u8> = b"hello";
        let a: address = @0x0;
        let (x, y): (&u64, &mut u64) = (&0, &mut 1);
        let S { f, g: f2 }: S = S { f: 0, g: 1 };
    }
}
```

Note that the type annotations must always be to the right of the pattern:

```move
// ERROR! should be let (x, y): (&u64, &mut u64) = ...
let (x: &u64, y: &mut u64) = (&0, &mut 1);
```

### When annotations are necessary

In some cases, a local type annotation is required if the type system cannot infer the type. This
commonly occurs when the type argument for a generic type cannot be inferred. For example:

```move
let _v1 = vector[]; // ERROR!
//        ^^^^^^^^ Could not infer this type. Try adding an annotation
let v2: vector<u64> = vector[]; // no error
```

In a rarer case, the type system might not be able to infer a type for divergent code (where all the
following code is unreachable). Both [`return`](./functions.md#return-expression) and
[`abort`](./abort-and-assert.md) are expressions and can have any type. A
[`loop`](./control-flow/loops.md) has type `()` if it has a `break` (or `T` if has a `break e` where
`e: T`), but if there is no break out of the `loop`, it could have any type. If these types cannot
be inferred, a type annotation is required. For example, this code:

```move
let a: u8 = return ();
let b: bool = abort 0;
let c: signer = loop ();

let x = return (); // ERROR!
//  ^ Could not infer this type. Try adding an annotation
let y = abort 0; // ERROR!
//  ^ Could not infer this type. Try adding an annotation
let z = loop (); // ERROR!
//  ^ Could not infer this type. Try adding an annotation
```

Adding type annotations to this code will expose other errors about dead code or unused local
variables, but the example is still helpful for understanding this problem.

### Multiple declarations with tuples

`let` can introduce more than one local at a time using tuples. The locals declared inside the
parenthesis are initialized to the corresponding values from the tuple.

```move
let () = ();
let (x0, x1) = (0, 1);
let (y0, y1, y2) = (0, 1, 2);
let (z0, z1, z2, z3) = (0, 1, 2, 3);
```

The type of the expression must match the arity of the tuple pattern exactly.

```move
let (x, y) = (0, 1, 2); // ERROR!
let (x, y, z, q) = (0, 1, 2); // ERROR!
```

You cannot declare more than one local with the same name in a single `let`.

```move
let (x, x) = 0; // ERROR!
```

The mutability of the local variables declared can be mixed.

```move
let (mut x, y) = (0, 1);
x = 1;
```

### Multiple declarations with structs

`let` can also introduce more than one local at a time when destructuring (or matching against) a
struct. In this form, the `let` creates a set of local variables that are initialized to the values
of the fields from a struct. The syntax looks like this:

```move
public struct T { f1: u64, f2: u64 }
```

```move
let T { f1: local1, f2: local2 } = T { f1: 1, f2: 2 };
// local1: u64
// local2: u64
```

Similarly for positional structs

```move
public struct P(u64, u64)
```

and

```move
let P(local1, local2) = T { f1: 1, f2: 2 };
// local1: u64
// local2: u64
```

Here is a more complicated example:

```move
module 0x42::example {
    public struct X(u64)
    public struct Y { x1: X, x2: X }

    fun new_x(): X {
        X(1)
    }

    fun example() {
        let Y { x1: X(f), x2 } = Y { x1: new_x(), x2: new_x() };
        assert!(f + x2.f == 2, 42);

        let Y { x1: X(f1), x2: X(f2) } = Y { x1: new_x(), x2: new_x() };
        assert!(f1 + f2 == 2, 42);
    }
}
```

Fields of structs can serve double duty, identifying the field to bind _and_ the name of the
variable. This is sometimes referred to as punning.

```move
let Y { x1, x2 } = e;
```

is equivalent to:

```move
let Y { x1: x1, x2: x2 } = e;
```

As shown with tuples, you cannot declare more than one local with the same name in a single `let`.

```move
let Y { x1: x, x2: x } = e; // ERROR!
```

And as with tuples, the mutability of the local variables declared can be mixed.

```move
let Y { x1: mut x1, x2 } = e;
```

Furthermore, the mutability of annotation can be applied to the punned fields. Giving the equivalent
example

```move
let Y { mut x1, x2 } = e;
```

### Destructuring against references

In the examples above for structs, the bound value in the let was moved, destroying the struct value
and binding its fields.

```move
public struct T { f1: u64, f2: u64 }
```

```move
let T { f1: local1, f2: local2 } = T { f1: 1, f2: 2 };
// local1: u64
// local2: u64
```

In this scenario the struct value `T { f1: 1, f2: 2 }` no longer exists after the `let`.

If you wish instead to not move and destroy the struct value, you can borrow each of its fields. For
example:

```move
let t = T { f1: 1, f2: 2 };
let T { f1: local1, f2: local2 } = &t;
// local1: &u64
// local2: &u64
```

And similarly with mutable references:

```move
let mut t = T { f1: 1, f2: 2 };
let T { f1: local1, f2: local2 } = &mut t;
// local1: &mut u64
// local2: &mut u64
```

This behavior can also work with nested structs.

```move
module 0x42::example {
    public struct X(u64)
    public struct Y { x1: X, x2: X }

    fun new_x(): X {
        X(1)
    }

    fun example() {
        let mut y = Y { x1: new_x(), x2: new_x() };

        let Y { x1: X(f), x2 } = &y;
        assert!(*f + x2.f == 2, 42);

        let Y { x1: X(f1), x2: X(f2) } = &mut y;
        *f1 = *f1 + 1;
        *f2 = *f2 + 1;
        assert!(*f1 + *f2 == 4, 42);
    }
}
```

### Ignoring Values

In `let` bindings, it is often helpful to ignore some values. Local variables that start with `_`
will be ignored and not introduce a new variable

```move
fun three(): (u64, u64, u64) {
    (0, 1, 2)
}
```

```move
let (x1, _, z1) = three();
let (x2, _y, z2) = three();
assert!(x1 + z1 == x2 + z2, 42);
```

This can be necessary at times as the compiler will warn on unused local variables

```move
let (x1, y, z1) = three(); // WARNING!
//       ^ unused local 'y'
```

### General `let` grammar

All of the different structures in `let` can be combined! With that we arrive at this general
grammar for `let` statements:

> _let-binding_ → **let** _pattern-or-list_ _type-annotation_<sub>_opt_</sub> >
> _initializer_<sub>_opt_</sub> > _pattern-or-list_ → _pattern_ | **(** _pattern-list_ **)** >
> _pattern-list_ → _pattern_ **,**<sub>_opt_</sub> | _pattern_ **,** _pattern-list_ >
> _type-annotation_ → **:** _type_ _initializer_ → **=** _expression_

The general term for the item that introduces the bindings is a _pattern_. The pattern serves to
both destructure data (possibly recursively) and introduce the bindings. The pattern grammar is as
follows:

> _pattern_ → _local-variable_ | _struct-type_ **{** _field-binding-list_ **}** >
> _field-binding-list_ → _field-binding_ **,**<sub>_opt_</sub> | _field-binding_ **,** >
> _field-binding-list_ > _field-binding_ → _field_ | _field_ **:** _pattern_

A few concrete examples with this grammar applied:

```move
    let (x, y): (u64, u64) = (0, 1);
//       ^                           local-variable
//       ^                           pattern
//          ^                        local-variable
//          ^                        pattern
//          ^                        pattern-list
//       ^^^^                        pattern-list
//      ^^^^^^                       pattern-or-list
//            ^^^^^^^^^^^^           type-annotation
//                         ^^^^^^^^  initializer
//  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ let-binding

    let Foo { f, g: x } = Foo { f: 0, g: 1 };
//      ^^^                                    struct-type
//            ^                                field
//            ^                                field-binding
//               ^                             field
//                  ^                          local-variable
//                  ^                          pattern
//               ^^^^                          field-binding
//            ^^^^^^^                          field-binding-list
//      ^^^^^^^^^^^^^^^                        pattern
//      ^^^^^^^^^^^^^^^                        pattern-or-list
//                      ^^^^^^^^^^^^^^^^^^^^   initializer
//  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ let-binding
```

## Mutations

### Assignments

After the local is introduced (either by `let` or as a function parameter), a `mut` local can be
modified via an assignment:

```move
x = e
```

Unlike `let` bindings, assignments are expressions. In some languages, assignments return the value
that was assigned, but in Move, the type of any assignment is always `()`.

```move
(x = e: ())
```

Practically, assignments being expressions means that they can be used without adding a new
expression block with braces (`{`...`}`).

```move
let x;
if (cond) x = 1 else x = 2;
```

The assignment uses the similar pattern syntax scheme as `let` bindings, but with absence of `mut`:

```move
module 0x42::example {
    public struct X { f: u64 }

    fun new_x(): X {
        X { f: 1 }
    }

    // Note: this example will complain about unused variables and assignments.
    fun example() {
       let (mut x, mut y, mut f, mut g) = (0, 0, 0, 0);

       (X { f }, X { f: x }) = (new_x(), new_x());
       assert!(f + x == 2, 42);

       (x, y, f, _, g) = (0, 0, 0, 0, 0);
    }
}
```

Note that a local variable can only have one type, so the type of the local cannot change between
assignments.

```move
let mut x;
x = 0;
x = false; // ERROR!
```

### Mutating through a reference

In addition to directly modifying a local with assignment, a `mut` local can be modified via a
mutable reference `&mut`.

```move
let mut x = 0;
let r = &mut x;
*r = 1;
assert!(x == 1, 42);
```

This is particularly useful if either:

(1) You want to modify different variables depending on some condition.

```move
let mut x = 0;
let mut y = 1;
let r = if (cond) &mut x else &mut y;
*r = *r + 1;
```

(2) You want another function to modify your local value.

```move
let mut x = 0;
modify_ref(&mut x);
```

This sort of modification is how you modify structs and vectors!

```move
let mut v = vector[];
vector::push_back(&mut v, 100);
assert!(*vector::borrow(&v, 0) == 100, 42);
```

For more details, see [Move references](./primitive-types/references.md).

## Scopes

Any local declared with `let` is available for any subsequent expression, _within that scope_.
Scopes are declared with expression blocks, `{`...`}`.

Locals cannot be used outside of the declared scope.

```move
let x = 0;
{
    let y = 1;
};
x + y // ERROR!
//  ^ unbound local 'y'
```

But, locals from an outer scope _can_ be used in a nested scope.

```move
{
    let x = 0;
    {
        let y = x + 1; // valid
    }
}
```

Locals can be mutated in any scope where they are accessible. That mutation survives with the local,
regardless of the scope that performed the mutation.

```move
let mut x = 0;
x = x + 1;
assert!(x == 1, 42);
{
    x = x + 1;
    assert!(x == 2, 42);
};
assert!(x == 2, 42);
```

### Expression Blocks

An expression block is a series of statements separated by semicolons (`;`). The resulting value of
an expression block is the value of the last expression in the block.

```move
{ let x = 1; let y = 1; x + y }
```

In this example, the result of the block is `x + y`.

A statement can be either a `let` declaration or an expression. Remember that assignments (`x = e`)
are expressions of type `()`.

```move
{ let x; let y = 1; x = 1; x + y }
```

Function calls are another common expression of type `()`. Function calls that modify data are
commonly used as statements.

```move
{ let v = vector[]; vector::push_back(&mut v, 1); v }
```

This is not just limited to `()` types---any expression can be used as a statement in a sequence!

```move
{
    let x = 0;
    x + 1; // value is discarded
    x + 2; // value is discarded
    b"hello"; // value is discarded
}
```

But! If the expression contains a resource (a value without the `drop` [ability](./abilities.md)),
you will get an error. This is because Move's type system guarantees that any value that is dropped
has the `drop` [ability](./abilities.md). (Ownership must be transferred or the value must be
explicitly destroyed within its declaring module.)

```move
{
    let x = 0;
    Coin { value: x }; // ERROR!
//  ^^^^^^^^^^^^^^^^^ unused value without the `drop` ability
    x
}
```

If a final expression is not present in a block---that is, if there is a trailing semicolon `;`,
there is an implicit [unit `()` value](https://en.wikipedia.org/wiki/Unit_type). Similarly, if the
expression block is empty, there is an implicit unit `()` value.

Both are equivalent

```move
{ x = x + 1; 1 / x; }
```

```move
{ x = x + 1; 1 / x; () }
```

Similarly both are equivalent

```move
{ }
```

```move
{ () }
```

An expression block is itself an expression and can be used anyplace an expression is used. (Note:
The body of a function is also an expression block, but the function body cannot be replaced by
another expression.)

```move
let my_vector: vector<vector<u8>> = {
    let mut v = vector[];
    vector::push_back(&mut v, b"hello");
    vector::push_back(&mut v, b"goodbye");
    v
};
```

(The type annotation is not needed in this example and only added for clarity.)

### Shadowing

If a `let` introduces a local variable with a name already in scope, that previous variable can no
longer be accessed for the rest of this scope. This is called _shadowing_.

```move
let x = 0;
assert!(x == 0, 42);

let x = 1; // x is shadowed
assert!(x == 1, 42);
```

When a local is shadowed, it does not need to retain the same type as before.

```move
let x = 0;
assert!(x == 0, 42);

let x = b"hello"; // x is shadowed
assert!(x == b"hello", 42);
```

After a local is shadowed, the value stored in the local still exists, but will no longer be
accessible. This is important to keep in mind with values of types without the
[`drop` ability](./abilities.md), as ownership of the value must be transferred by the end of the
function.

```move
module 0x42::example {
    public struct Coin has store { value: u64 }

    fun unused_coin(): Coin {
        let x = Coin { value: 0 }; // ERROR!
//          ^ This local still contains a value without the `drop` ability
        x.value = 1;
        let x = Coin { value: 10 };
        x
//      ^ Invalid return
    }
}
```

When a local is shadowed inside a scope, the shadowing only remains for that scope. The shadowing is
gone once that scope ends.

```move
let x = 0;
{
    let x = 1;
    assert!(x == 1, 42);
};
assert!(x == 0, 42);
```

Remember, locals can change type when they are shadowed.

```move
let x = 0;
{
    let x = b"hello";
    assert!(x = b"hello", 42);
};
assert!(x == 0, 42);
```

## Move and Copy

All local variables in Move can be used in two ways, either by `move` or `copy`. If one or the other
is not specified, the Move compiler is able to infer whether a `copy` or a `move` should be used.
This means that in all of the examples above, a `move` or a `copy` would be inserted by the
compiler. A local variable cannot be used without the use of `move` or `copy`.

`copy` will likely feel the most familiar coming from other programming languages, as it creates a
new copy of the value inside of the variable to use in that expression. With `copy`, the local
variable can be used more than once.

```move
let x = 0;
let y = copy x + 1;
let z = copy x + 2;
```

Any value with the `copy` [ability](./abilities.md) can be copied in this way, and will be copied
implicitly unless a `move` is specified.

`move` takes the value out of the local variable _without_ copying the data. After a `move` occurs,
the local variable is unavailable, even if the value's type has the `copy`
[ability](./abilities.md).

```move
let x = 1;
let y = move x + 1;
//      ------ Local was moved here
let z = move x + 2; // Error!
//      ^^^^^^ Invalid usage of local 'x'
y + z
```

### Safety

Move's type system will prevent a value from being used after it is moved. This is the same safety
check described in [`let` declaration](#let-bindings) that prevents local variables from being used
before it is assigned a value.

<!-- For more information, see TODO future section on ownership and move semantics. -->

### Inference

As mentioned above, the Move compiler will infer a `copy` or `move` if one is not indicated. The
algorithm for doing so is quite simple:

- Any value with the `copy` [ability](./abilities.md) is given a `copy`.
- Any reference (both mutable `&mut` and immutable `&`) is given a `copy`.
  - Except under special circumstances where it is made a `move` for predictable borrow checker
    errors. This will happen once the reference is no longer used.
- Any other value is given a `move`.

Given the structs

```move
public struct Foo has copy, drop, store { f: u64 }
public struct Coin has store { value: u64 }
```

we have the following example

```move
let s = b"hello";
let foo = Foo { f: 0 };
let coin = Coin { value: 0 };
let coins = vector[Coin { value: 0 }, Coin { value: 0 }];

let s2 = s; // copy
let foo2 = foo; // copy
let coin2 = coin; // move
let coins2 = coin; // move

let x = 0;
let b = false;
let addr = @0x42;
let x_ref = &x;
let coin_ref = &mut coin2;

let x2 = x; // copy
let b2 = b; // copy
let addr2 = @0x42; // copy
let x_ref2 = x_ref; // copy
let coin_ref2 = coin_ref; // copy
```
