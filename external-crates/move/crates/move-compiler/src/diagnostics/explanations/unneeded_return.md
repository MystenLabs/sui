In tail position the `return` keyword is redundant — the trailing expression is already the
function's value.

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
