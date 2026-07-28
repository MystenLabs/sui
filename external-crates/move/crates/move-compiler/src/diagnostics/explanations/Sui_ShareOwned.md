Sharing is irreversible and makes an object world-writable forever. Calling `share_object` on an
object that is not freshly created in this function (it arrives as a parameter or from an unpack)
can silently hand every account write access to something a user believed was theirs. It fires only
when the type is also transferable — it has `store`, or is transferred with `transfer::transfer`
elsewhere; a `key`-only type that is never transferred is not flagged.

This lint is on by default.

## When it's OK

The shared object really is fresh, but was produced through a helper call the local analysis can't
see through — a conservative false positive.

## Example

Flagged:

```move
// `o` is a parameter — the checker can't prove it is fresh
public fun share(o: Obj) {
    transfer::public_share_object(o)
}
```

Suggested:

```move
// packed here, so provably a fresh object
public fun share(ctx: &mut TxContext) {
    transfer::share_object(Obj { id: object::new(ctx) })
}
```
