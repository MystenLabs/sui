# Conditional `if` Expressions

An `if` expression specifies that some code should only be evaluated if a certain condition is true.
For example:

```move
if (x > 5) x = x - 5
```

The condition must be an expression of type `bool`.

An `if` expression can optionally include an `else` clause to specify another expression to evaluate
when the condition is false.

```move
if (y <= 10) y = y + 1 else y = 10
```

Either the "true" branch or the "false" branch will be evaluated, but not both. Either branch can be
a single expression or an expression block.

The conditional expressions may produce values so that the `if` expression has a result.

```move
let z = if (x < 100) x else 100;
```

The expressions in the true and false branches must have compatible types. For example:

```move=
// x and y must be u64 integers
let maximum: u64 = if (x > y) x else y;

// ERROR! branches different types
let z = if (maximum < 10) 10u8 else 100u64;

// ERROR! branches different types, as default false-branch is () not u64
if (maximum >= 10) maximum;
```

If the `else` clause is not specified, the false branch defaults to the unit value. The following
are equivalent:

```move
if (condition) true_branch // implied default: else ()
if (condition) true_branch else ()
```

Commonly, `if` expressions are used in conjunction with
[expression blocks](../variables.md#expression-blocks).

```move
let maximum = if (x > y) x else y;
if (maximum < 10) {
    x = x + 10;
    y = y + 10;
} else if (x >= 10 && y >= 10) {
    x = x - 10;
    y = y - 10;
}
```

## Grammar for Conditionals

> _if-expression_ → **if (** _expression_ **)** _expression_ _else-clause_<sub>_opt_</sub> >
> _else-clause_ → **else** _expression_
