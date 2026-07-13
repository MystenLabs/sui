A struct whose first field is `id: UID` looks like a Sui object, but without the `key` ability it
can never be stored, transferred, or shared as one. This is almost always a forgotten `has key`.

This lint is on by default.

## Example

Flagged:

```move
public struct MissingKeyAbility {
    id: UID,
}
```

Suggested:

```move
public struct HasKeyAbility has key {
    id: UID,
}
```
