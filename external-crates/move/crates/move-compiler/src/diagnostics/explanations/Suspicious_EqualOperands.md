A binary operation whose two operands are the same expression has an already-known result: `x == x`
is always `true`, `x - x` is `0`, `x / x` is `1`, `x & x` is just `x`. The operation is dead weight
and often a copy-paste bug where one side should differ.

This lint is off by default; enable it with `--lint`.

## Example

Flagged:

```move
let b = x == x;
```

Suggested:

```move
let b = true;
```
