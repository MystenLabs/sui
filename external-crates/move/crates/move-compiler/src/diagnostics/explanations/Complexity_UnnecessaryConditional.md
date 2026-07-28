An `if` with one branch `true` and the other `false` collapses to the condition itself (or its
negation), and one whose branches are the same literal value collapses to that value. The
conditional only adds noise.

This lint is off by default; enable it with `--lint`.

## Example

Flagged:

```move
let b = if (!condition) true else false;
```

Suggested:

```move
let b = !condition;
```
