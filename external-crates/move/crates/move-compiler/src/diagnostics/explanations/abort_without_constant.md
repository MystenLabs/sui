A bare numeric abort code carries no meaning at the call site or in error output. A named constant
documents the failure and keeps codes consistent across the module.

## Example

Flagged:

```move
abort 100
```

Suggested:

```move
const EInvalidArgument: u64 = 1;
// ...
abort EInvalidArgument
```
