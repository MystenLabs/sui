An object can become shared only in the transaction that created it. Sharing an object that may
already be address-owned aborts. This lint flags `share_object` and `public_share_object` calls when
the argument was not packed locally and its type can be owned: either the type has `store`, or the
compiler finds a private transfer of that type.

## When it's OK

An object produced by a helper may still be fresh in the current transaction, but the local analysis
cannot see through the call. This is a conservative false positive.

A `key`-only type with no `store` and no private transfer call cannot be address-owned through the
transfer API. The checker does not report that case.

## Example

Flagged:

```move
public struct Obj has key, store {
    id: UID,
}

// `o` may already be address-owned.
public fun share(o: Obj) {
    transfer::public_share_object(o)
}
```
