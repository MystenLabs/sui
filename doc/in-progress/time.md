# Accessing Time in Move

## `sui::clock::Clock`

Transactions that need access to an accurate clock must pass a read-only reference of `sui::clock::Clock` in as an entry function parameter.  The only available instance is found at address `0x6`.

A unix timestamp in milliseconds can be extracted from an instance of `Clock` using

```
module sui::clock {
    public fun timestamp_ms(clock: &Clock): u64;
}
```

**The resulting value is expected to change every 2 to 3 seconds**, at the rate that checkpoints are committed by the network.

Successive calls to `sui::clock::timestamp_ms` in the same transaction will always produce the same result (transactions are considered to take effect instantly), and may also overlap with other transactions that touch the same shared objects, so it is not safe to assume that time progresses between transactions that are sequenced relative to each other.

Any transaction that requires access to a `Clock` must go through **consensus**, because the only available instance is a **shared object**, so this technique is not suitable for transactions that must use the single-owner fast-path (see "Epoch timestamps" for a single-owner-compatible source of timestamps).

**Transactions that use the clock must accept it as an immutable reference** (not a mutable reference or value).  This prevents contention, as transactions that access the `Clock` can only read it, so do not need to be sequenced relative to each other.  Validators will refuse to sign transactions that do not meet this requirement and packages that include entry functions that accept a `Clock` or `&mut Clock` will fail to publish.

Code that depends on `Clock` can be tested using

```
module sui::clock {
    #[test_only]
    public fun increment_for_testing(clock: &mut Clock, tick: u64);
}
```

which can increment the timestamp in the `Clock` in test code.


## Epoch timestamps

All transactions (including ones that do not go through consensus) can access the timestamp for the start of the current epoch, via the following function:

```
module sui::tx_context {
    public fun epoch_timestamp_ms(ctx: &TxContext): u64;
}
```

Which returns the point in time when the current epoch started, as a millisecond granularity unix timestamp in a `u64`.  **This value changes roughly once every 24 hours**, when the Epoch changes.

The value returned by this function can be updated in tests for time-sensitive code that use `sui::test_scenario`, using

```
module sui::test_scenario {
    public fun later_epoch(
        scenario: &mut Scenario,
        delta_ms: u64,
        sender: address,
    ): TransactionEffects;
}
```

which behaves like `sui::test_scenario::next_epoch` (finishes the current transaction and epoch), but also increments the timestamp by `delta_ms` milliseconds.
