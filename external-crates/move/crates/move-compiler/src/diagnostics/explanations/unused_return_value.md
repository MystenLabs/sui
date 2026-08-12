Discarding the result of a call that has no `&mut` arguments means the call did nothing observable.
Either use the value or make the intent explicit with `let _ = ...`. In Sui mode `&mut TxContext` is
treated as non-mutating, so such calls are still flagged.

## When it's OK

Calls that take a `&mut` argument (a real side effect) are not flagged.

## Example

Flagged:

```move
fun price(x: u64): u64 { x + 1 }
// ...
price(10);
```

Suggested:

```move
let _ = price(10);
```
