Two comparisons over the same operand pair joined by `&&`/`||` often collapse to a single comparison
(`x == y && x >= y` is just `x == y`), or to a constant `true`/`false` (`x >= y || x <= y`).

## When it's OK

Negated comparisons are not handled and won't fire.

## Example

Flagged:

```move
let b = x == y && x >= y;
```

Suggested:

```move
let b = x == y;
```
