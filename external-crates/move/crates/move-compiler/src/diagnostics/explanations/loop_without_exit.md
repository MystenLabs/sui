A `loop` whose body contains neither `break` nor `return` has no normal exit — it runs until it
aborts, if ever. This is almost always a missing exit condition. (`while` is covered separately by
`while_true`.)

## Example

Flagged:

```move
let i = 0;
loop {
    i = i + 1;
}
```

Suggested:

```move
let i = 0;
loop {
    if (i >= 10) break;
    i = i + 1;
}
```
