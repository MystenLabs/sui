A public function that takes `&TxContext` cannot later create objects — which needs `&mut TxContext`
— without a breaking signature change. Taking `&mut TxContext` up front keeps the API
upgrade-compatible.

## Example

Flagged:

```move
public fun incorrect_mint(_ctx: &TxContext) {}
```

Suggested:

```move
public fun correct_mint(_ctx: &mut TxContext) {}
```
