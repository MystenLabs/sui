A bare numeric abort code carries no meaning at the call site or in error output. A named constant
documents the failure and keeps codes consistent across the module.

This lint is off by default; enable it with `--lint`.

## When it's OK

The whole argument must be a single named constant — `abort A + B` still fires. `assert!(cond,
ECode)` is the idiomatic form.

## Example

Flagged:

```move
abort 100
```

Suggested:

```move
const ERR_INVALID_ARGUMENT: u64 = 1;
// ...
abort ERR_INVALID_ARGUMENT
```
