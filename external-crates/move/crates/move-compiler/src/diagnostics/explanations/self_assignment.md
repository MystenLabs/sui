Assigning a location to itself (`x = x`, `*r = *r`, `s.f = s.f`) has no effect. It usually signals a
typo or an unfinished edit.

## Example

Flagged:

```move
p = p;
```

Suggested:

```move
// remove the redundant statement
```
