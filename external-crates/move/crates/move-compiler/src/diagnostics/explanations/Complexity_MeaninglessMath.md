An operation with an identity operand (`* 1`, `/ 1`, `+ 0`, `- 0`, `<< 0`, `>> 0`) does nothing; one
with an absorbing operand (`* 0`, `0 / x`, `x % 1`, `0 % x`) is `0`; and `1 % x` is `1`. Only
literal `0`/`1` operands are detected.

This lint is off by default; enable it with `--lint`.

## Example

Flagged:

```move
let y = x * 1;
```

Suggested:

```move
let y = x;
```
