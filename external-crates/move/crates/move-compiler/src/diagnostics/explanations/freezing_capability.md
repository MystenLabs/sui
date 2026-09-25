Capabilities gate privileged actions. Freezing one turns it into a permanent immutable object that
anyone can reference and that can never be revoked — usually the opposite of the intended access
control. The lint matches by struct name (a capitalized `Cap`).

## When it's OK

The value is not actually an authority-bearing capability, or making it permanently immutable is a
deliberate and safe part of the design.

## Example

Flagged:

```move
public struct AdminCap has key { id: UID }

public fun freeze_cap(cap: AdminCap) {
    transfer::public_freeze_object(cap)
}
```

Suggested:

```move
// keep the capability owned instead of freezing it
public fun keep_cap(cap: AdminCap, owner: address) {
    transfer::transfer(cap, owner)
}
```
