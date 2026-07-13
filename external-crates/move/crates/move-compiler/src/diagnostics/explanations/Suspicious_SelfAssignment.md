Assigning a location to itself (`x = x`, `*r = *r`, `s.f = s.f`) has no effect. It usually signals a
typo or an unfinished edit.

This lint is off by default; enable it with `--lint`.

## Example

Flagged:

```move
p = p;
```

Suggested:

```move
// remove the redundant statement
```
