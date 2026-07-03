# SIP-70: Current Command Range Hash

## Summary

Add one native function on `sui::tx_context`:

```move
public fun current_command_range_hash(ctx: &TxContext, n: u64): vector<u8>
```

This returns a versioned hash of the current PTB command and the next `n - 1`
commands.

The point is simple: let a contract lock in the part of a PTB it cares about,
while leaving the rest of the PTB open for wallets, solvers, sponsors, or other
composition.

This is more useful than a whole-PTB hash for smart accounts and governance,
because the authorized intent can be fixed without forcing the whole transaction
to be fixed.

## Motivation

Sui PTBs are already the right execution model for composition. The missing
piece is that Move cannot currently ask:

> "Does the next part of this PTB match the thing I authorized?"

Without that primitive, apps that need separated authorization and execution
build wrappers, interpreters, action specs, or custom typed execution systems.
That works, but it moves PTB semantics into application code.

`current_command_range_hash(ctx, n)` keeps the primitive at the PTB layer:

- fixed intent commands can be committed to
- solver/wallet commands before or after the range can stay open
- Move receives only an opaque hash
- no PTB introspection is exposed
- no sub-PTB command is added

This is idiomatic because it makes fixed intents and open solver execution
composable without leaking PTB abstractions into Move.

## Specification

### Move API

```move
module sui::tx_context {
    /// Returns a hash of the current command and the next `n - 1` commands.
    /// Output: [version_byte | blake2b256_hash] (33 bytes).
    public fun current_command_range_hash(_self: &TxContext, n: u64): vector<u8>;
}
```

`n` must be greater than zero.

The current command is the PTB command that is currently executing this native.
For example, if command 4 calls a smart account function that calls
`current_command_range_hash(ctx, 3)`, the hash covers commands 4, 5, and 6.

### Output Format

```text
[version_byte | blake2b256_hash]
```

- version `0x01` = current command range hash scheme
- total output: 33 bytes

If fewer than `n` commands remain in the PTB, the function returns a
domain-separated unavailable hash, not an abort. The unavailable hash must not
collide with any valid command range hash. Callers should treat it as a normal
hash comparison failure.

This lets contracts safely write:

```move
assert!(
    tx_context::current_command_range_hash(ctx, n) == expected_hash,
    EHashMismatch,
);
```

without also needing to introspect PTB length.

### What Is Hashed

The hash commits to the selected contiguous command range only.

For every command in the range, the encoder commits to:

- command kind
- package, module, function, and type arguments for Move calls
- command-specific data for non-Move-call commands
- argument count and argument structure
- result flow between commands inside the range
- imported values from outside the range, as imports

All strings, byte blobs, and lists are length-framed.

### What Is Not Hashed

The hash does not commit to:

- sender/caller address
- sponsor address
- signatures
- transaction digest
- gas budget
- gas price
- gas coin object ID
- gas coin version
- commands before the selected range
- commands after the selected range
- absolute command position

The VM may use the current command index internally to compute the range, but
the API exposes only the hash. It does not expose position, depth, frame kind, or
whether the caller is inside any higher-level application abstraction.

### Argument Normalization

Each PTB argument is normalized before hashing:

| Argument Type | What Gets Hashed | Rationale |
| --- | --- | --- |
| `GasCoin` | marker only | gas coin identity is sender-dependent and should not be part of intent |
| `Pure(bytes)` | exact bytes | fixed parameter value |
| `SharedObject` | ObjectID + mutability mode | stable object anchor plus lock semantics |
| `ImmOrOwnedObject` | ObjectID | version can drift between authorization and execution |
| `Receiving` | ObjectID | version can drift |
| result inside range | relative command/result index | commits to flow inside the locked range |
| result before range | import slot | allows solver/wallet setup before the locked range |

Direct PTB inputs are fixed. If a command in the range uses a direct object or
pure input, that value is part of the hash.

If a command in the range uses a value produced by a command before the range,
that value is encoded as an import. The hash commits that an imported value is
used, and where it is used, but not to the full command sequence that produced
it.

Import slots are assigned by first use inside the range. Reusing the same
external value reuses the same import slot. Different external values get
different import slots.

This is the key composability property: a solver can do any number of setup
commands first, then route values into the fixed range, without changing the
range hash.

### Hash Construction

Conceptually:

```text
range_hash = 0x01 || Blake2b256(
    "SUI_CURRENT_COMMAND_RANGE_HASH"
    || n
    || for each selected command in order:
        Blake2b256(
            command_kind
            || length_framed(command_specific_data)
            || normalized_arguments
        )
)
```

The concrete encoder should be semantic and versioned, not raw BCS of the PTB
Rust type. Future PTB shape changes can then be handled by the encoder instead
of changing the Move API.

### Example

The wallet or solver can build:

```text
0. solver setup command
1. solver setup command
2. smart_account::authorize(expected_hash, 3, ctx)
3. fixed intent command
4. fixed intent command
5. solver settlement command
```

Inside command 2, the smart account checks:

```move
let actual = tx_context::current_command_range_hash(ctx, 3);
assert!(actual == expected_hash, EHashMismatch);
```

The hash covers commands 2, 3, and 4.

Commands 0, 1, and 5 are not hashed. The solver can change them without
changing the authorized intent, as long as the locked range still receives the
right imported values and has the same command structure.

## Why Not Full PTB Hash

A full PTB hash is too rigid for the main use case.

Smart accounts and governance usually want to authorize the important part of a
transaction, not the exact gas setup, solver route, sponsorship details, or
settlement tail.

Whole-PTB commitment makes every solver/wallet implementation detail part of the
authorization. That is hard to use and hard to support.

Range hash keeps the protocol primitive smaller:

- no new PTB command
- no nested PTB execution
- no full PTB introspection
- no dependency on commands outside the authorized range
- no sender/gas coupling

## Implementation Notes

This can be implemented with the same broad machinery as a structural digest,
but scoped to the current command range:

1. Store or expose the current `ProgrammableTransaction` to `TxContext` during
   execution.
2. Track the currently executing command index internally in the PTB executor.
3. When the native is called, hash `[current_index, current_index + n)`.
4. Encode in-range result references relatively.
5. Encode references to earlier results as imports.
6. Return the unavailable hash if the requested range runs past the end.
7. Charge base gas plus a per-byte/per-command cost for the hashed range.

The current command index is execution metadata. It is not returned to Move and
is not part of the public API.

## Security Considerations

- VM-computed and read-only
- order-sensitive within the selected range
- flow-sensitive within the selected range
- no sender/caller address or gas coin identity in the hash
- no PTB introspection
- no way to ask for absolute PTB position
- commands outside the range remain intentionally open

Contracts should compare the returned hash against an expected hash that was
computed off-chain using the same versioned encoder.

## Backwards Compatibility

Purely additive. This adds one new native function behind a protocol version.

The version byte lets future hash schemes coexist with existing stored hashes.

## Future Work

- SDK support for off-chain range hash computation
- test vectors for common smart-account and solver patterns
- optional helper APIs in wallets for building PTBs around a locked range
