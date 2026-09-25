A `public` function that takes `Random` or `RandomGenerator` can be called by other Move code,
letting an attacker draw randomness and react to the outcome within the same transaction. Randomness
should only be reachable from a transaction, not composed by another contract.

## Example

Flagged:

```move
public fun not_allowed(_r: &Random) {}
```

Suggested:

```move
entry fun basic_random(_r: &Random) {}
```
