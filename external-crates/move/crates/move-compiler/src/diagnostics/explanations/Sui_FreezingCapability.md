Capabilities gate privileged actions. Freezing one turns it into a permanent immutable object that
anyone can reference and that can never be revoked — usually the opposite of the intended access
control. The lint matches by struct name (a capitalized `Cap`).

This lint is off by default; enable it with `--lint`.

## When it's OK

The match is by name — a `Cap` at the end of the name, or followed by an uppercase letter, digit, or
`_`. So a non-capability like `NoCap` is a false positive, while a real capability with an
off-pattern name (`AdminRights`, `Capv0`) is missed.

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
