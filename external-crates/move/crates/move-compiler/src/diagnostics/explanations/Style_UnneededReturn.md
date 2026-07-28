In tail position the `return` keyword is redundant — the trailing expression is already the
function's value.

This lint is off by default; enable it with `--lint`.

## When it's OK

Only a tail-position `return` is flagged — of any value-yielding expression (a call, `S { .. }`,
arithmetic, a cast, `loop`, even `()`). A `return` used as an early exit, or whose operand doesn't
yield a value (e.g. `return abort E`), is left alone.

## Example

Flagged:

```move
fun price(): u64 {
    return 5
}
```

Suggested:

```move
fun price(): u64 {
    5
}
```
