# Pattern Matching

A `match` expression is a powerful control structure that allows you to compare a value against a
series of patterns and then execute code based on which pattern matches first. Patterns can be
anything from simple literals to complex, nested struct and enum definitions . As opposed to `if` expressions, which change control flow based on a `bool`-typed test expression, a `match` expression operates over
a value of any type and selects on of many arms.

A `match` expression can match Move values, immutable references, or mutable references, binding
sub-patterns accordingly.

A pattern is matched by a value if the value is equal to the pattern, and where variables and
wildcards (e.g., `x`, `y`, `_`, or `..`) are "equal" to anything.

For example:

```move
fun run(x: u64): u64 {
    match (x) {
        1 => 2,
        2 => 3,
        x => x,
    }
}

run(1); // returns 2
run(2); // returns 3
run(3); // returns 3
run(0); // returns 0
```

## Syntax

A `match` takes an expression and a non-empty series of _match arms_ delimited by commas.

Each match arm consists of a pattern `p`, an optional guard `if (g)` where `g` is an expression of
type `bool`, an arrow `=>`, and an arm expression `e` to execute when the pattern matches. E.g.,

```move
match (expression) {
    pattern1 if (guard_expression) => expression1,
    pattern2 => expression2,
    pattern3 => { expression3, expression4, ... },
}
```

Match arms are checked in order from top to bottom, and the first pattern which matches
(with a guard expression, if present, that evaluates to `true`) will be executed.

Note that the series of match arms within a `match` must be exhaustive, meaning that every possible
value of the type being matched must be covered by one of the patterns in the `match`. If the series
of match arms is not exhaustive, the compiler will raise an error.

## Patterns

Patterns are used to match values. Patterns can be

- literals (`true`, `2`, `@0x4`);
- constants (`MyConstant`);
- variables (`a`, `b`, `x`);
- wildcards (`_`);
- constructor patterns (`MyStruct { a, b }`, `MyEnum::Variant(x)`);
- at-patterns `<variable> @ <pattern>`; and
- or-patterns `<pattern> | <pattern>`.

Additionally, depending on the context patterns may also include:

- multi-arity wildcards (`..`); and
- mutable-binding patterns (`mut x`).

Some examples of patterns are:

```move
public enum MyEnum {
    Variant(u64, bool),
    OtherVariant(bool, u64),
}

public enum OtherEnum {
    V(MyEnum)
}

public struct MyStruct {
    x: u64,
    y: u64,
}

// literal pattern
1

// constant pattern
MyConstant

// variable pattern
x

// wildcard pattern
_

// constructor pattern that matches `MyEnum::Variant` with the fields `1` and `true`
MyEnum::Variant(1, true)

// constructor pattern that matches `MyEnum::Variant` with the fields `1` and binds the second field's value to `x`
MyEnum::Variant(1, x)

// multi-arity wildcard pattern that matches multiple fields within the `MyEnum::Variant` variant
MyEnum::Variant(..)

// constructor pattern that matches the `x` field of `MyStruct` and binds the `y` field to `other_variable`
MyStruct { x, y: other_variable }

// at-pattern that matches `MyEnum::Variant` and binds the entire value to `x`
x @ MyEnum::Variant(..)

// or-pattern that matches either `MyEnum::Variant` or `MyEnum::OtherVariant`
MyEnum::Variant(..) | MyEnum::OtherVariant(..)

// Same as the above or-pattern, but with explicit wildcards
MyEnum::Variant(_, _) | MyEnum::OtherVariant(_, _)

// or-pattern that matches either `MyEnum::Variant` or `MyEnum::OtherVariant` and binds the u64 field to `x`
MyEnum::Variant(x, _) | MyEnum::OtherVariant(_, x)

// constructor pattern that matches `OtherEnum::V` and if the inner `MyEnum` is `MyEnum::Variant`
OtherEnum::V(MyEnum::Variant(..))
```

More concisely we have the following grammar for patterns in Move:

```bnf
pattern = <literal>
        | <constant>
        | <variable>
        | _
        | C { <variable> : inner-pattern ["," <variable> : inner-pattern]* } // where C is a struct or enum variant
        | C ( inner-pattern ["," inner-pattern]* ... ) // where C is a struct or enum variant
        | C                                       // where C is an enum variant
        | <variable> @ top-level-pattern
        | pattern | pattern
inner-pattern = pattern
              | ..
              | mut <variable>
```

Patterns that contain variables bind them to the match subject or subject subcomponent being matched.
These variables can then be
used either in any match guard expressions, or on the right-hand side of the match arm. For example:

```move
public struct Wrapper(u64)

fun add_under_wrapper_unless_equal(wrapper: Wrapper, x: u64): u64 {
    match (wrapper) {
        Wrapper(y) if (y == x) => Wrapper(y),
        Wrapper(y) => y + x,
    }
}
add_under_wrapper_unless_equal(Wrapper(1), 2); // returns Wrapper(3)
add_under_wrapper_unless_equal(Wrapper(2), 3); // returns Wrapper(5)
add_under_wrapper_unless_equal(Wrapper(3), 3); // returns Wrapper(3)
```

Patterns can be nested, and patterns can be combined used the or operator `|` which will succeed if either
pattern matches. The `..` pattern is a special
pattern that matches any number of fields in a struct or enum variant, but it can only occur within
a constructor pattern, similarly the `mut` pattern can only be used within constructor patterns --
this is used to specify that we want to use the variable mutably on the right-hand-side of the match
arm.

Patterns are not expressions, but they are nevertheless typed. This means that
the type of a pattern must match the type of the value it matches. For example, the pattern `1` has
type `u64`, the pattern `MyEnum::Variant(1, true)` has type `MyEnum`, and the pattern
`MyStruct { x, y }` has type `MyStruct`. If you try to match on an expression which differs from the
type of the pattern in the match this will result in a type error. For example:

```move
match (1) {
    // The `true` literal pattern is of type `bool` so this is a type error.
    true => 1,
    // TYPE ERROR: expected type u64, found bool
    _ => 2,
}
```

Similarly the following would also result in a type error since `MyEnum` and `MyStruct` are
different types:

```
match (MyStruct { x: 0, y: 0 }) {
    // TYPE ERROR: expected type MyEnum, found MyStruct
    MyEnum::Variant(..) => 1,
}
```

Additionally, there are some restrictions on when the `..` pattern, and `mut` pattern modifier can
be used in a pattern.

A `mut` modifier can only occur within a constructor pattern, and cannot be a top-level pattern. The
value being matched on must be either a mutable reference or by value in order for a `mut` pattern
to be used.

```move
public struct MyStruct(u64)

fun top_level_mut(x: MyStruct) {
    match (x) {
        mut MyStruct(y) => 1,
        // ERROR: cannot use mut pattern as a top-level pattern
    }
}

fun mut_on_non_mut(x: MyStruct): u64 {
    match (x) {
        // OK! Since `x` is matched by value
        MyStruct(mut y) =>  {
            *y = *y + 1;
            *y
        },
    }
}

fun mut_on_mut(x: &mut MyStruct): u64 {
    match (x) {
        // OK! Since `x` is matched by mutable reference
        MyStruct(mut y) =>  {
            *y = *y + 1;
            *y
        },
    }
}

let mut x = MyStruct(1);
mut_on_non_mut(&mut x); // returns 2
x.0; // returns 2

fun mut_on_immut(x: &MyStruct): u64 {
    match (x) {
        MyStruct(mut y) => ...,
        // ERROR: cannot use mut pattern on a non-mutable reference
    }
}
```

The `..` pattern an only be used within a constructor pattern and:

- It can only be used **once** within the constructor pattern;
- In positional arguments it can be used at the beginning, middle, or end of the patterns within the
  constructor;
- In named arguments it can only be used at the end of the patterns within the constructor;

```move
public struct MyStruct(u64, u64, u64, u64) has drop;

public struct MyStruct2 {
    x: u64,
    y: u64,
    z: u64,
    w: u64,
}

fun wild_match(x: MyStruct) {
    match (x) {
        MyStruct(.., 1) => 1,
        // OK! The `..` pattern can be used at the begining of the constructor pattern
        MyStruct(1, ..) => 2,
        // OK! The `..` pattern can be used at the end of the constructor pattern
        MyStruct(1, .., 1) => 3,
        // OK! The `..` pattern can be used at the middle of the constructor pattern
        MyStruct(1, .., 1, 1) => 4,
        MyStruct(..) => 5,
    }
}

fun wild_match2(x: MyStruct2) {
    match (x) {
        MyStruct2 { x: 1, .. } => 1,
        MyStruct2 { x: 1, w: 2 .. } => 2,
        MyStruct2 { .. } => 3,
    }
}
```

## Matching

Prior to delving into the specifics of pattern matching and what it means for a value to "match" a
pattern, let's examine a few examples to provide an intuition for the concept.

```move
fun test_lit(x: u64): u8 {
    match (x) {
        1 => 2,
        2 => 3,
        _ => 4,
    }
}
test_lit(1); // returns 2
test_lit(2); // returns 3
test_lit(3); // returns 4
test_lit(10); // returns 4

fun test_var(x: u64): u64 {
    match (x) {
        y => y,
    }
}
test_var(1); // returns 1
test_var(2); // returns 2
test_var(3); // returns 3
...

const MyConstant: u64 = 10;
fun test_constant(x: u64): u64 {
    match (x) {
        MyConstant => 1,
        _ => 2,
    }
}
test_constant(MyConstant); // returns 1
test_constant(10); // returns 1
test_constant(20); // returns 2

fun test_or_pattern(x: u64): u64 {
    match (x) {
        1 | 2 | 3 => 1,
        4 | 5 | 6 => 2,
        _ => 3,
    }
}
test_or_pattern(1); // returns 1
test_or_pattern(2); // returns 1
test_or_pattern(3); // returns 1
test_or_pattern(4); // returns 2
test_or_pattern(5); // returns 2
test_or_pattern(6); // returns 2
test_or_pattern(7); // returns 3
test_or_pattern(70); // returns 3

fun test_or_at_pattern(x: u64): u64 {
    match (x) {
        x @ (1 | 2 | 3) => x + 1,
        y @ (4 | 5 | 6) => y + 2,
        z => z + 3,
    }
}
test_or_pattern(1); // returns 2
test_or_pattern(2); // returns 3
test_or_pattern(3); // returns 4
test_or_pattern(4); // returns 6
test_or_pattern(5); // returns 7
test_or_pattern(6); // returns 8
test_or_pattern(7); // returns 10
test_or_pattern(70); // returns 73
```

The most important thing to note from these examples is that a pattern matches a value if the value
is equal to the pattern, and wildcard/variable patterns match anything. This is true for literals,
variables, and constants. For example, in the `test_lit` function, the value `1` matches the pattern
`1`, the value `2` matches the pattern `2`, and the value `3` matches the wildcard `_`. Similarly,
in the `test_var` function, the value `1` matches the pattern `y` and the value `2` matches the
pattern `y`.

A variable `x` matches (or "equals") any value, and a wildcard `_` matches any value (but only one
value!). Or-patterns are like a logical OR, where a value matches the pattern if it matches any of
patterns in the or-pattern so `p1 | p2 | p3` should be read "matches p1, or p2, or p3".

The most interesting part of pattern matching are constructor patterns. These patterns allow you
inspect and access deep within both structs and enums, and are the most powerful part of pattern
matching. Constructor patterns, coupled with variable bindings, allow you to match on values by
their structure, and pull out the parts of the value you care about for usage on the right-hand side
of the match arm.

Take the following:

```move
fun f(x: MyEnum) {
    match (x) {
        MyEnum::Variant(1, true) => 1,
        MyEnum::OtherVariant(_, 3) => 2,
        MyEnum::Variant(..) => 3,
        MyEnum::OtherVariant(..) => 4,
}
f(MyEnum::Variant(1, true)); // returns 1
f(MyEnum::Variant(2, true)); // returns 3
f(MyEnum::OtherVariant(false, 3)); // returns 2
f(MyEnum::OtherVariant(true, 3)); // returns 2
f(MyEnum::OtherVariant(true, 2)); // returns 4
```

This is saying that "if `x` is `MyEnum::Variant` with the fields `1` and `true`, then return `1`, if
it is `MyEnum::OtherVariant` with any value for the first field, and `3` for the second, then return
`2`, if it is `MyEnum::Variant` with any fields, then return `3`, and if it is
`MyEnum::OtherVariant` with any fields, then return `4`".

You can also nest patterns, so if I wanted to match either 1, 2, or 10, instead of just matching 1
in the `MyEnum::Variant` above, you could do so with an or-pattern:

```move
fun f(x: MyEnum) {
    match (x) {
        MyEnum::Variant(1 | 2 | 10, true) => 1,
        MyEnum::OtherVariant(_, 3) => 2,
        MyEnum::Variant(..) => 3,
        MyEnum::OtherVariant(..) => 4,
}
f(MyEnum::Variant(1, true)); // returns 1
f(MyEnum::Variant(2, true)); // returns 1
f(MyEnum::Variant(10, true)); // returns 1
f(MyEnum::Variant(10, false)); // returns 3
```

Additionally, match bindings are subject to the same ability restrictions as other aspects of Move. In particular, the compiler will signal an error if you try to match a value (i.e., not-reference) without `drop` using a wildcard, as the wildcard expects to drop the value. Similarly, if you bind a non-`drop` value using a binder, it must be used in the right-hand side of the match arm. In addition, if you fully-destruct that value, you have unpacked it, matching the semantics of  [non-`drop` struct unpacking](link). See [ref section] for more details about the `drop` capability. 

```move
public struct NonDrop(u64)

fun drop_nondrop(x: NonDrop) {
    match (x) {
        NonDrop(1) => 1,
        _ => 2
        // ERROR: cannot wildcard match on a non-droppable value
    }
}

fun destructure_nondrop(x: NonDrop) {
    match (x) {
        NonDrop(1) => 1,
        NonDrop(_) => 2
        // OK!
    }
}

fun use_nondrop(x: NonDrop): NonDrop {
    match (x) {
        NonDrop(1) => NonDrop(8),
        x => x
    }
}
```

## Exhaustiveness

The `match` expression in Move must be _exhaustive_: every possible value of the type being matched
must be covered by one of the patterns in one of the match's arms. If the series of match arms is
not exhaustive, the compiler will raise an error. Note that any arm with a guard expression
does not contribute to match exhaustion, as it may fail to match at runtime.

As an example, if we were to match on a `u8` then in order for the match to be exhaustive we would
need to match on _every_ number from 0 to 255 inclusive, or a wildcard or variable pattern would need
to be present. Similarly if we were to match on a `bool` then we would need to match on both `true`
and `false`, or a wildcard or variable pattern would need to be present.

For structs, since there is only one type of constructor for the type, only one constructor needs to
be matched, but the fields within the struct need to be matched exhaustively as well. Conversely,
enums may define multiple variants, and each variant must be matched (including any sub-fields) in order for the match to be
considered exhaustive.

Since underscores and variables match anything, they count as matching all values of the type they
are matching on in that position. Additionally, the multi-arity wildcard pattern `..` can be used to
match on multiple values within a struct or enum variant.

To see some examples of _non-exhaustive_ matches, consider the following:

```move
public enum MyEnum {
    Variant(u64, bool),
    OtherVariant(bool, u64),
}

public struct Pair<T>(T, T)

fun f(x: MyEnum): u8 {
    match (x) {
        MyEnum::Variant(1, true) => 1,
        MyEnum::Variant(_, _) => 1,
        MyEnum::OtherVariant(_, 3) => 2,
        // ERROR: not exhaustive as the value `MyEnum::OtherVariant(_, 4)` is not matched.
    }
}

fun match_pair_bool(x: Pair<bool>): u8 {
    match (x) {
        Pair(true, true) => 1,
        Pair(true, false) => 1,
        Pair(false, false) => 1,
        // ERROR: not exhaustive as the value `Pair(false, true)` is not matched.
    }
}
```

These examples can then be made exhaustive by adding a wildcard pattern to the end of the match arm,
or by fully matching on the remaining values:

```move
fun f(x: MyEnum): u8 {
    match (x) {
        MyEnum::Variant(1, true) => 1,
        MyEnum::Variant(_, _) => 1,
        MyEnum::OtherVariant(_, 3) => 2,
        // Now exhaustive since this will match all values of MyEnum::OtherVariant
        MyEnum::OtherVariant(..) => 2,

    }
}

fun match_pair_bool(x: Pair<bool>): u8 {
    match (x) {
        Pair(true, true) => 1,
        Pair(true, false) => 1,
        Pair(false, false) => 1,
        // Now exhaustive since this will match all values of Pair<bool>
        Pair(false, true) => 1,
    }
}
```

## Guards

As mentioned above you can add a guard to a match arm by adding an `if` clause after the pattern.
This guard will run _after_ the pattern has been matched but _before_ the expression on the
right-hand-side of the arrow is evaluated. If the guard expression evaluates to `true` then the
expression on the right-hand side of the arrow will be evaluated, if it evaluates to `false` then it
will be considered a "failed match" and the next match arm in the `match` expression will be
checked.

```move
fun match_with_guard(x: u64): u64 {
    match (x) {
        1 if (x == 0) => 1,
        1 => 2,
        _ => 3,
    }
}

match_with_guard(1); // returns 2
match_with_guard(0); // returns 3
```

Guard expressions can reference variables bound in the pattern during evaluation.
However, note that _variables are only available as immutable reference in guards_ regardless
of the pattern being matched -- even if there are mutability specifiers on the variable or if the
pattern is being matched by value.

```move
fun incr(x: &mut u64) {
    *x = *x + 1;
}

fun match_with_guard_incr(x: u64): u64 {
    match (x) {
        x if ({ incr(&mut x); x == 1 }) => 1,
        // ERROR:    ^^^ invalid borrow of immutable value
        _ => 2,
    }
}

fun match_with_guard_incr2(x: &mut u64): u64 {
    match (x) {
        x if ({ incr(&mut x); x == 1 }) => 1,
        // ERROR:    ^^^ invalid borrow of immutable value
        _ => 2,
    }
}
```

Additionally, it is important to note any match arms that have guard expressions will not be
considered either for exhaustivity purposes since the compiler has no way of evaluating the guard
expression statically.
