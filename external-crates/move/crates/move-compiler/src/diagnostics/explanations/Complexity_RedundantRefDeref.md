Taking a reference and immediately dereferencing it (`&*(&x)`), or dereferencing a fresh field
borrow (`*(&s.f)`), is a no-op the compiler already performs for you.

This lint is off by default; enable it with `--lint`.

## When it's OK

Dereferencing an existing reference (`*r`) is fine — only dereferencing a fresh borrow (`*(&x)` or
`&*(&x)`) is redundant.

## Example

Flagged:

```move
let _ref = &*(&resource);
```

Suggested:

```move
let _ref = &resource;
```
