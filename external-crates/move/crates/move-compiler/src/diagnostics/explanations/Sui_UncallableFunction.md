The transaction runtime can only supply certain argument shapes. Taking `TxContext` by value, a
`&mut TxContext` alongside any other `TxContext` parameter (it must be the only one), or a `&mut` to
`Clock`/`Random` makes the function impossible to call from a transaction.

This lint is on by default.

## When it's OK

A single `&mut TxContext` is fine. Use `&Clock` and `&Random` rather than `&mut`.

## Example

Flagged:

```move
// `Clock` must be passed by immutable reference
fun uses_clock(_c: &mut Clock) {}
```

Suggested:

```move
fun uses_clock(_c: &Clock) {}
```
