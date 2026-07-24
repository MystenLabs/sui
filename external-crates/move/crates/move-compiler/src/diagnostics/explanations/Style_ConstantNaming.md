Constants are expected to be `UPPER_SNAKE_CASE` or PascalCase (either is accepted for any constant).
A lowercase or mixed name (`max_supply`, `JSON_Max_Size`) reads like a variable and breaks module
consistency.

This lint is off by default; enable it with `--lint`.

## When it's OK

PascalCase is deliberately allowed — including the `E`-prefixed PascalCase used for error constants,
such as `ENotAuthorized`.

## Example

Flagged:

```move
const Another_BadName: u64 = 42;
```

Suggested:

```move
const MAX_LIMIT: u64 = 1000;
const ENotAuthorized: u64 = 0;
```
