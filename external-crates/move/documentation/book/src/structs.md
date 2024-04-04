# Structs and Resources

A _struct_ is a user-defined data structure containing typed fields. Structs can store any
non-reference, non-tuple type, including other structs.

Structs can be used to define all "asset" values or unrestricted values, where the operations
performed on those values can be controlled by the struct's [abilities](./abilities.md). By default,
structs are linear and ephemeral. By this we mean that they: cannot be copied, cannot be dropped,
and cannot be stored in storage. This means that all values have to have ownership transferred
(linear) and the values must be dealt with by the end of the program's execution (ephemeral). We can
relax this behavior by giving the struct [abilities](./abilities.md) which allow values to be copied
or dropped and also to be stored in storage or to define storage schemas.

## Defining Structs

Structs must be defined inside a module, and the struct's fields can either be named or positional:

```move
module a::m {
    public struct Foo { x: u64, y: bool }
    public struct Bar {}
    public struct Baz { foo: Foo, }
    //                          ^ note: it is fine to have a trailing comma

    public struct PosFoo(u64, bool)
    public struct PosBar()
    public struct PosBaz(Foo)
}
```

Structs cannot be recursive, so the following definitions are invalid:

```move
public struct Foo { x: Foo }
//                     ^ ERROR! recursive definition

public struct A { b: B }
public struct B { a: A }
//                   ^ ERROR! recursive definition

public struct D(D)
//              ^ ERROR! recursive definition
```

### Visibility

As you may have noticed, all structs are declared as `public`. This means that the type of the
struct can be referred to from any other module. However, the fields of the struct, and the ability
to create or destroy the struct, are still internal to the module that defines the struct.

In the future, we plan on adding to declare structs as `public(package)` or as internal, much like
[functions](./functions.md#visibility).

### Abilities

As mentioned above: by default, a struct declaration is linear and ephemeral. So to allow the value
to be used in these ways (e.g., copied, dropped, stored in an [object](./abilities/object.md), or
used to define a storable [object](./abilities/object.md)), structs can be granted
[abilities](./abilities.md) by annotating them with `has <ability>`:

```move
module a::m {
    public struct Foo has copy, drop { x: u64, y: bool }
}
```

The ability declaration can occur either before or after the struct's fields. However, only one or
the other can be used, and not both. If declared after the struct's fields, the ability declaration
must be terminated with a semicolon:

```move
module a::m {
    public PreNamedAbilities has copy, drop { x: u64, y: bool }
    public struct PostNamedAbilities { x: u64, y: bool } has copy, drop;
    public struct PostNamedAbilitiesInvalid { x: u64, y: bool } has copy, drop
    //                                                                        ^ ERROR! missing semicolon

    public struct NamedInvalidAbilities has copy { x: u64, y: bool } has drop;
    //                                                               ^ ERROR! duplicate ability declaration

    public PrePositionalAbilities has copy, drop (u64, bool)
    public struct PostPositionalAbilities (u64, bool) has copy, drop;
    public struct PostPositionalAbilitiesInvalid (u64, bool) has copy, drop
    //                                                                     ^ ERROR! missing semicolon
    public struct InvalidAbilities has copy (u64, bool) has drop;
    //                                                  ^ ERROR! duplicate ability declaration
}
```

For more details, see the section on
[annotating a struct's abilities](./abilities.md#annotating-structs).

### Naming

Structs must start with a capital letter `A` to `Z`. After the first letter, struct names can
contain underscores `_`, letters `a` to `z`, letters `A` to `Z`, or digits `0` to `9`.

```move
public struct Foo {}
public struct BAR {}
public struct B_a_z_4_2 {}
public struct P_o_s_Foo()
```

This naming restriction of starting with `A` to `Z` is in place to give room for future language
features. It may or may not be removed later.

## Using Structs

### Creating Structs

Values of a struct type can be created (or "packed") by indicating the struct name, followed by
value for each field.

For a struct with named fields, the order of the fields does not matter, but the field name needs to
be provided. For a struct with positional fields, the order of the fields must match the order of
the fields in the struct definition, and it must be created using `()` instead of `{}` to enclose
the parameters.

```move
module a::m {
    public struct Foo has drop { x: u64, y: bool }
    public struct Baz has drop { foo: Foo }
    public struct Positional(u64, bool) has drop;

    fun example() {
        let foo = Foo { x: 0, y: false };
        let baz = Baz { foo: foo };
        // Note: positional struct values are created using parentheses and
        // based on position instead of name.
        let pos = Positional(0, false);
        let pos_invalid = Positional(false, 0);
        //                           ^ ERROR! Fields are out of order and the types don't match.
    }
}
```

For structs with named fields, you can use the following shorthand if you have a local variable with
the same name as the field:

```move
let baz = Baz { foo: foo };
// is equivalent to
let baz = Baz { foo };
```

This is sometimes called "field name punning".

### Destroying Structs via Pattern Matching

Struct values can be destroyed by binding or assigning them in patterns using similar syntax to
constructing them.

```move
module a::m {
    public struct Foo { x: u64, y: bool }
    public struct Bar(Foo)
    public struct Baz {}
    public struct Qux()

    fun example_destroy_foo() {
        let foo = Foo { x: 3, y: false };
        let Foo { x, y: foo_y } = foo;
        //        ^ shorthand for `x: x`

        // two new bindings
        //   x: u64 = 3
        //   foo_y: bool = false
    }

    fun example_destroy_foo_wildcard() {
        let foo = Foo { x: 3, y: false };
        let Foo { x, y: _ } = foo;

        // only one new binding since y was bound to a wildcard
        //   x: u64 = 3
    }

    fun example_destroy_foo_assignment() {
        let x: u64;
        let y: bool;
        Foo { x, y } = Foo { x: 3, y: false };

        // mutating existing variables x and y
        //   x = 3, y = false
    }

    fun example_foo_ref() {
        let foo = Foo { x: 3, y: false };
        let Foo { x, y } = &foo;

        // two new bindings
        //   x: &u64
        //   y: &bool
    }

    fun example_foo_ref_mut() {
        let foo = Foo { x: 3, y: false };
        let Foo { x, y } = &mut foo;

        // two new bindings
        //   x: &mut u64
        //   y: &mut bool
    }

    fun example_destroy_bar() {
        let bar = Bar(Foo { x: 3, y: false });
        let Bar(Foo { x, y }) = bar;
        //            ^ nested pattern

        // two new bindings
        //   x: u64 = 3
        //   y: bool = false
    }

    fun example_destroy_baz() {
        let baz = Baz {};
        let Baz {} = baz;
    }

    fun example_destroy_qux() {
        let qux = Qux();
        let Qux() = qux;
    }
}
```

### Accessing Struct Fields

Fields of a struct can be accessed using the dot operator `.`.

For structs with named fields, the fields can be accessed by their name:

```move
public struct Foo { x: u64, y: bool }
let foo = Foo { x: 3, y: true };
let x = foo.x;  // x == 3
let y = foo.y;  // y == true
```

For positional structs, fields can be accessed by their position in the struct definition:

```move
public struct PosFoo(u64, bool)
let pos_foo = PosFoo(3, true);
let x = pos_foo.0;  // x == 3
let y = pos_foo.1;  // y == true
```

Accessing struct fields without borrowing or copying them is subject to the field's ability
constraints. For more details see the sections on
[borrowing structs and fields](#borrowing-structs-and-fields) and
[reading and writing fields](#reading-and-writing-fields) for more information.

### Borrowing Structs and Fields

The `&` and `&mut` operator can be used to create references to structs or fields. These examples
include some optional type annotations (e.g., `: &Foo`) to demonstrate the type of operations.

```move
let foo = Foo { x: 3, y: true };
let foo_ref: &Foo = &foo;
let y: bool = foo_ref.y;         // reading a field via a reference to the struct
let x_ref: &u64 = &foo.x;        // borrowing a field by extending a reference to the struct

let x_ref_mut: &mut u64 = &mut foo.x;
*x_ref_mut = 42;            // modifying a field via a mutable reference
```

It is possible to borrow inner fields of nested structs:

```move
let foo = Foo { x: 3, y: true };
let bar = Bar(foo);

let x_ref = &bar.0.x;
```

You can also borrow a field via a reference to a struct:

```move
let foo = Foo { x: 3, y: true };
let foo_ref = &foo;
let x_ref = &foo_ref.x;
// this has the same effect as let x_ref = &foo.x
```

### Reading and Writing Fields

If you need to read and copy a field's value, you can then dereference the borrowed field:

```move
let foo = Foo { x: 3, y: true };
let bar = Bar(copy foo);
let x: u64 = *&foo.x;
let y: bool = *&foo.y;
let foo2: Foo = *&bar.0;
```

More canonically, the dot operator can be used to read fields of a struct without any borrowing. As
is true with
[dereferencing](./primitive-types/references.md#reading-and-writing-through-references), the field
type must have the `copy` [ability](./abilities.md).

```move
let foo = Foo { x: 3, y: true };
let x = foo.x;  // x == 3
let y = foo.y;  // y == true
```

Dot operators can be chained to access nested fields:

```move
let bar = Bar(Foo { x: 3, y: true });
let x = baz.0.x; // x = 3;
```

However, this is not permitted for fields that contain non-primitive types, such a vector or another
struct:

```move
let foo = Foo { x: 3, y: true };
let bar = Bar(foo);
let foo2: Foo = *&bar.0;
let foo3: Foo = bar.0; // error! must add an explicit copy with *&
```

We can mutably borrow a field to a struct to assign it a new value:

```move
let mut foo = Foo { x: 3, y: true };
*&mut foo.x = 42;     // foo = Foo { x: 42, y: true }
*&mut foo.y = !foo.y; // foo = Foo { x: 42, y: false }
let mut bar = Bar(foo);               // bar = Bar(Foo { x: 42, y: false })
*&mut bar.0.x = 52;                   // bar = Bar(Foo { x: 52, y: false })
*&mut bar.0 = Foo { x: 62, y: true }; // bar = Bar(Foo { x: 62, y: true })
```

Similar to dereferencing, we can instead directly use the dot operator to modify a field. And in
both cases, the field type must have the `drop` [ability](./abilities.md).

```move
let mut foo = Foo { x: 3, y: true };
foo.x = 42;     // foo = Foo { x: 42, y: true }
foo.y = !foo.y; // foo = Foo { x: 42, y: false }
let mut bar = Bar(foo);         // bar = Bar(Foo { x: 42, y: false })
bar.0.x = 52;                   // bar = Bar(Foo { x: 52, y: false })
bar.0 = Foo { x: 62, y: true }; // bar = Bar(Foo { x: 62, y: true })
```

The dot syntax for assignment also works via a reference to a struct:

```move
let foo = Foo { x: 3, y: true };
let foo_ref = &mut foo;
foo_ref.x = foo_ref.x + 1;
```

## Privileged Struct Operations

Most struct operations on a struct type `T` can only be performed inside the module that declares
`T`:

- Struct types can only be created ("packed"), destroyed ("unpacked") inside the module that defines
  the struct.
- The fields of a struct are only accessible inside the module that defines the struct.

Following these rules, if you want to modify your struct outside the module, you will need to
provide public APIs for them. The end of the chapter contains some examples of this.

However as stated [in the visibility section above](#visibility), struct _types_ are always visible
to another module

```move
module a::m {
    public struct Foo has drop { x: u64 }

    public fun new_foo(): Foo {
        Foo { x: 42 }
    }
}

module a::n {
    use a::m::Foo;

    public struct Wrapper has drop {
        foo: Foo
        //   ^ valid the type is public

    }

    fun f1(foo: Foo) {
        let x = foo.x;
        //      ^ ERROR! cannot access fields of `Foo` outside of `a::m`
    }

    fun f2() {
        let foo_wrapper = Wrapper { foo: m::new_foo() };
        //                               ^ valid the function is public
    }
}

```

## Ownership

As mentioned above in [Defining Structs](#defining-structs), structs are by default linear and
ephemeral. This means they cannot be copied or dropped. This property can be very useful when
modeling real world assets like money, as you do not want money to be duplicated or get lost in
circulation.

```move
module a::m {
    public struct Foo { x: u64 }

    public fun copying() {
        let foo = Foo { x: 100 };
        let foo_copy = copy foo; // ERROR! 'copy'-ing requires the 'copy' ability
        let foo_ref = &foo;
        let another_copy = *foo_ref // ERROR! dereference requires the 'copy' ability
    }

    public fun destroying_1() {
        let foo = Foo { x: 100 };

        // error! when the function returns, foo still contains a value.
        // This destruction requires the 'drop' ability
    }

    public fun destroying_2(f: &mut Foo) {
        *f = Foo { x: 100 } // error!
                            // destroying the old value via a write requires the 'drop' ability
    }
}
```

To fix the example `fun destroying_1`, you would need to manually "unpack" the value:

```move
module a::m {
    public struct Foo { x: u64 }

    public fun destroying_1_fixed() {
        let foo = Foo { x: 100 };
        let Foo { x: _ } = foo;
    }
}
```

Recall that you are only able to deconstruct a struct within the module in which it is defined. This
can be leveraged to enforce certain invariants in a system, for example, conservation of money.

If on the other hand, your struct does not represent something valuable, you can add the abilities
`copy` and `drop` to get a struct value that might feel more familiar from other programming
languages:

```move
module a::m {
    public struct Foo has copy, drop { x: u64 }

    public fun run() {
        let foo = Foo { x: 100 };
        let foo_copy = foo;
        //             ^ this code copies foo,
        //             whereas `let x = move foo` would move foo

        let x = foo.x;            // x = 100
        let x_copy = foo_copy.x;  // x = 100

        // both foo and foo_copy are implicitly discarded when the function returns
    }
}
```

## Storage

Structs can be used to define storage schemas, but the details are different per deployment of Move.
See the documentation for the [`key` ability](./abilities.md#key) and
[Sui objects](./abilities/object.md) for more details.
