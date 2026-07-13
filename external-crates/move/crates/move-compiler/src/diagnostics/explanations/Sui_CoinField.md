`Coin<T>` is itself a full object (it has an `id: UID`) meant for transfers between accounts. Held
as a field it adds needless object plumbing; `Balance<T>` is the storage-oriented type for keeping
value inside another object.

This lint is on by default.

## When it's OK

Keep `Coin` only if the field genuinely needs to be an independent object. An alias does not avoid
the lint — the resolved type is what's matched.

## Example

Flagged:

```move
public struct S2 has key, store {
    id: UID,
    c: Coin<S1>,
}
```

Suggested:

```move
public struct S2 has key, store {
    id: UID,
    c: Balance<S1>,
}
```
