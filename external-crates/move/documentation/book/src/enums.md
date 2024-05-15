# Enumerations

An _enum_ is a user-defined data structure containing one-or-more _variants_. Each variant can
optionally contain typed fields. The number, and types of these fields can differ for each variant
in the enumeration. Fields in enums can store any non-reference, non-tuple type, including other
structs or enums.

As a simple example, consider the following enum definition in Move:

```move
public enum Action {
    Stop,
    Pause { duration: u32 },
    MoveTo { x: u64, y: u64 },
    Jump(u64),
}
```

This declares an enum `Action` that represents different actions that can be taken by a game -- you
can `Stop`, `Pause` for a given duration, `MoveTo` a specific location, or `Jump` to a specific
height.

Similarly to structs, enums can have [abilities](./abilities.md) that control what operations can be
performed on them. It is important to note however that enums cannot have the `key` ability since
they cannot be top-level objects.

## Defining Enums

Enums must be defined in a module, an enum must contain at least on variant, and each variant of an
enum can either have no fields, positional fields, or named fields. Here are some examples of each:

```move
module a::m {
    public enum Foo has drop {
        VariantWithNoFields,
        //                 ^ note: it is fine to have a trailing comma after variant declarations
    }
    public enum Bar has copy, drop {
        VariantWithPositionalFields(u64, bool),
    }
    public enum Baz has drop {
        VariantWithNamedFields { x: u64, y: bool, z: Bar },
    }
}
```

Enums cannot be recursive in any of their variants, so the following definitions of an enum are not
allowed since they would be recursive in at least one variant:

```move
module a::m {
    public enum Foo {
        Recursive(Foo),
        //        ^ error: recursive enum variant
    }
    public enum List {
        Nil,
        Cons { head: u64, tail: List },
        //                      ^ error: recursive enum variant
    }
    public enum BTree<T> {
        Leaf(T),
        Node { left: BTree<T>, right: BTree<T> },
        //           ^ error: recursive enum variant
    }
}
```

## Visibility

All enums are declared as `public`. This means that the type of the enum can be referred to from any
other module. However, the variants of the enum, the fields within each variant, and the ability to
create or destroy variants of the enum are internal to the module that defines the enum.

In the future, we plan on adding the ability to declare enums to have public variants, but this is
currently not supported.

### Abilities

Just like with structs, by default an enum declaration is linear and ephemeral. In order to use an
enum value in a non-linear or non-ephemeral way -- i.e., copied, dropped, or stored in an
[object](./abilities/object.md) -- you need to grant it additional [abilities](./abilities.md) by
annotating them with `has <ability>`:

```move
module a::m {
    public enum Foo has copy, drop {
        VariantWithNoFields,
    }
}
```

The ability declaration can occur either before or after the enum's variants, however only one or
the other can be used, and not both. If declared after the variants, the ability declaration must be
terminated with a semicolon:

```move
module a::m {
    public enum PreNamedAbilities has copy, drop { Variant }
    public enum PostNamedAbilities { Variant } has copy, drop;
    public enum PostNamedAbilitiesInvalid { Variant } has copy, drop
    //                                                              ^ ERROR! missing semicolon

    public enum NamedInvalidAbilities has copy { Variant } has drop;
    //                                                     ^ ERROR! duplicate ability declaration
}
```

For more details, see the section on
[annotating abilities](./abilities.md#annotating-structs-and-enums).

## Naming

Enums and variants within enums must start with a capital letter `A` to `Z`. After the first letter,
enum names can contain underscores `_`, letters `a` to `z`, letters `A` to `Z`, or digits `0` to
`9`.

```move
public enum Foo { Variant }
public enum BAR { Variant }
public enum B_a_z_4_2 { V_a_riant_0 }
```

This naming restriction of starting with `A` to `Z` is in place to give room for future language
features. It may or may not be removed later.

## Using Enums

### Creating Enum Variants

Values of an enum type can be created (or "packed") by indicating a variant of the enum, followed by
a value for each field in the variant. The variant name must always be qualified by the enum's name.

Similarly to structs, for a variant with named fields, the order of the fields does not matter but
the field names need to be provided. For a variant with positional fields, the order of the fields
matters and the order of the fields must match the order in the variant declaration and it must be
created using `()` instead of `{}`. If the variant has no fields, the variant name is sufficient and
no `()` or `{}` needs to be used.

```move
module a::m {
    public enum Action has drop {
        Stop,
        Pause { duration: u32 },
        MoveTo { x: u64, y: u64 },
        Jump(u64),
    }
    public enum Other has drop {
        Stop(u64),
    }

    fun example() {
        // Note: The `Stop` variant of `Action` doesn't have fields so no parentheses or curlies are needed.
        let stop = Action::Stop;
        let pause = Action::Pause { duration: 10 };
        let move_to = Action::MoveTo { x: 10, y: 20 };
        let jump = Action::Jump(10);
        // Note: The `Stop` variant of `Other` does have positional fields so we need to supply them.
        let other_stop = Other::Stop(10);
    }
}
```

For variants with named fields you can also use the shorthand syntax that you might be familiar with
from structs to create the variant:

```move
let duration = 10;

let pause = Action::Pause { duration: duration };
// is equivalent to
let pause = Action::Pause { duration };
```

### Pattern Matching Enum Variants and Destructuring

Since enum values can take on different shapes, dot access to fields of variants is not allowed like
it is for struct fields. Instead, in order to access fields within a variant -- either by value, or
immutable or mutable reference -- you must use pattern matching.

You can pattern match on Move values by value, immutable reference, and mutable reference. When
pattern matching by value, the value is moved into the match arm. When pattern matching by
reference, the value is borrowed into the match arm (either immutably or mutably). We'll go through
a brief description of pattern matching using `match` here, but for more information on pattern
matching using `match` in Move see the [Pattern Matching](./pattern_matching.md) section.

A `match` statement is used to pattern match on a Move value and consists of a number of _match
arms_. Each match arm consists of a pattern, an arrow `=>`, and an expression, followed by a comma
`,`. The pattern can be a struct, enum variant, binding (`x`, `y`), wildcard (`_` or `..`), constant
(`ConstValue`), or literal value (`true`, `42` etc). The value is matched against each pattern from
the top-down, and will match the first pattern that structurally matches the value. Once the value
is matched the expression on the right hand side of the `=>` is executed.

Additionally, match arms can have optional _guards_ that are checked after the pattern matches but
_before_ the expression is executed. Guards are specified by the `if` keyword followed by an
expression that must evaluate to a boolean value before the `=>`.

```move
module a::m {
    public enum Action has drop {
        Stop,
        Pause { duration: u32 },
        MoveTo { x: u64, y: u64 },
        Jump(u64),
    }

    public struct GameState {
        // Fields containing a game state
        character_x: u64,
        character_y: u64,
        character_height: u64,
        // ...
    }

    fun perform_action(stat: &mut GameState, action: Action) {
        match (action) {
            // Handle the `Stop` variant
            Action::Stop => state.stop(),
            // Handle the `Pause` variant
            // If the duration is 0, do nothing
            Action::Pause { duration: 0 } => (),
            Action::Pause { duration } => state.pause(duration),
            // Handle the `MoveTo` variant
            Action::MoveTo { x, y } => state.move_to(x, y),
            // Handle the `Jump` variant
            // if the game disallows jumps then do nothing
            Action::Jump(_) if (state.jumps_not_allowed()) => (),
            // otherwise, jump to the specified height
            Action::Jump(height) => state.jump(height),
        }
    }
}

```

To see how to pattern match on an enum to update values within it mutably lets take the following
example of a simple enum that has two variants, each with a single field. We can then write two
functions one that only increments the value of the first variant and another that only increments
the value of the second variant:

```move
module a::m {
    public enum SimpleEnum {
        Variant1(u64),
        Variant2(u64),
    }

    public fun incr_enum_variant1(simple_enum: &mut SimpleEnum) {
        match simple_enum {
            SimpleEnum::Variant1(mut value) => *value += 1,
            _ => (),
        }
    }

    public fun incr_enum_variant2(simple_enum: &mut SimpleEnum) {
        match simple_enum {
            SimpleEnum::Variant2(mut value) => *value += 1,
            _ => (),
        }
    }
}

```

Now, if we have a value of `SimpleEnum` we can use the functions to increment the value of this
variant:

```move
let mut x = SimpleEnum::Variant1(10);
incr_enum_variant1(&mut x);
assert!(x == SimpleEnum::Variant1(11));
// Doesn't increment since it increments a different variant
incr_enum_variant2(&mut x);
assert!(x == SimpleEnum::Variant1(11));
```

When pattern matching on Move value that does not have the `drop` ability, the value must be
consumed or destructured in each match arm. If the value is not consumed or destructured in a match
arm, the compiler will raise an error. This is to ensure that all possible values are handled in the
match statement.

As an example, consider the following code:

```move
module a::m {
    public enum X { Variant { x: u64 } }

    public fun bad(x: X) {
        match x {
            _ => ()
           // ^ ERROR! value of type `X` is not consumed or destructured in this match arm
        }
    }
}
```

In order to properly handle this, you will need to destructure `X` and all its variants in the
match's arm(s):

```move
module a::m {
    public enum X { Variant { x: u64 } }

    public fun good(x: X) {
        match x {
            // OK! Compiles since the value is destructured
            X::Variant { x: _ } => ()
        }
    }
}
```

### Overwriting to Enum Values

As long as the enum has the `drop` ability, you can overwrite the value of an enum with a new value
of the same type just as you might with other values in Move.

```move
module a::m {
    public enum X has drop {
        A(u64),
        B(u64),
    }

    public fun overwrite_enum(x: &mut X) {
        *x = X::A(10);
    }
}
```

```move
let mut x = X::B(20);
overwrite_enum(&mut x);
assert!(x == X::A(10));
```
