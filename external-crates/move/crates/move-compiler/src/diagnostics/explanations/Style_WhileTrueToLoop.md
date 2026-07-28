`while (true)` is an infinite loop written the long way. `loop` states the intent directly and can
`break` with a value. Only the literal `true` condition is detected.

This lint is off by default; enable it with `--lint`.

## Example

Flagged:

```move
while (true) {
    // ...
}
```

Suggested:

```move
loop {
    // ...
}
```
