A public function that takes `&TxContext` cannot later create objects — which needs `&mut TxContext`
— without a breaking signature change. Taking `&mut TxContext` up front keeps the API
upgrade-compatible.

This lint is off by default; enable it with `--lint`.

## When it's OK

Only `public` functions are checked; any non-`public` visibility (private, `public(package)`,
`public(friend)`) is exempt, since those signatures can change freely.

## Example

Flagged:

```move
public fun incorrect_mint(_ctx: &TxContext) {}
```

Suggested:

```move
public fun correct_mint(_ctx: &mut TxContext) {}
```
