Transferring an object (one with `key` + `store`) to `ctx.sender()` inside the function makes it
non-composable: a caller (or a programmable transaction block) cannot use the object because the
function gives it away internally. Returning the object lets the caller decide what to do with it.

This lint is on by default.

## When it's OK

Rare — the composable pattern is to return the object and let the caller place it. `entry` functions
and `init` are already exempt.

## Example

Flagged:

```move
// `S1` has `key` + `store`
public fun mint(ctx: &mut TxContext) {
    transfer::public_transfer(S1 { id: object::new(ctx) }, ctx.sender())
}
```

Suggested:

```move
public fun mint(ctx: &mut TxContext): S1 {
    S1 { id: object::new(ctx) }
}
```
