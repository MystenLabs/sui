`entry` on a `public` function is redundant: a `public` function is already callable from a
programmable transaction block. `entry` is only meaningful on a non-`public` function, where it is
what makes the function callable as a transaction command.

This lint is on by default.

## Example

Flagged:

```move
public entry fun mint() {}
```

Suggested:

```move
public fun mint() {}
```
