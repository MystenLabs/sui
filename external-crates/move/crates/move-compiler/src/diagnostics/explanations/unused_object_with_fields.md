A reference to an object that carries data beyond its `id`, but whose fields are never read, is a
likely mistake: the function takes the object yet never looks at the values it holds.

## When it's OK

Objects with no field beyond `id` (pure marker capabilities), by-value params, and generic object
types are out of scope. Otherwise the function should assert on or otherwise read a field — or drop
the parameter if it truly isn't needed.

## Example

Flagged:

```move
public struct OwnerCap has key { id: UID, owns: bool }

public fun unused(_c: &OwnerCap) {}
```

Suggested:

```move
const EDoesNotOwn: u64 = 0;

public fun assert_owns(c: &OwnerCap) {
    assert!(c.owns, EDoesNotOwn)
}
```
