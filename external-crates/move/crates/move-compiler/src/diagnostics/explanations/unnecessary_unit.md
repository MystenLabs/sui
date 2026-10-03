A `()` unit expression as a non-final statement, or as a branch of an `if`, adds nothing. Remove it,
or invert the condition to drop the empty branch.

## Example

Flagged:

```move
if (b) () else { x = 1 };
```

Suggested:

```move
if (!b) { x = 1 };
```
