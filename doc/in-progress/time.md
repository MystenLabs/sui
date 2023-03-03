# Accessing Time in Move

You have options when needing to access network-based time for your transactions. If you need a near real-time measurement (within a few seconds), use the immutable reference of time provided by the `Clock` module in Sui Move. The reference value from this module updates with every network checkpoint. If you don't need as current a time slice, use the `epoch_timestamp_ms` function to capture the precise moment the current epoch started.
## `sui::clock::Clock`

To access a prompt timestamp, you must pass a read-only reference of `sui::clock::Clock` as an entry function parameter in your transactions. The only available instance is found at address `0x6`.

Extract a unix timestamp in milliseconds from an instance of `Clock` using

```
module sui::clock {
    public fun timestamp_ms(clock: &Clock): u64;
}
```

**Expect the resulting value to change every 2 to 3 seconds**, at the rate the network commits checkpoints.

Successive calls to `sui::clock::timestamp_ms` in the same transaction always produce the same result (transactions are considered to take effect instantly), and might also overlap with other transactions that touch the same shared objects. Consequently, you can't assume that time progresses between transactions that are sequenced relative to each other.

Any transaction that requires access to a `Clock` must go through [consensus](/learn/architecture/consensus) because the only available instance is a [shared object](/learn/objects#shared). As a result, this technique is not suitable for transactions that must use the single-owner fast-path (see [Epoch timestamps](#epoch-timestamps) for a single-owner-compatible source of timestamps).

**Transactions that use the clock must accept it as an immutable reference** (not a mutable reference or value).  This prevents contention, as transactions that access the `Clock` can only read it, so do not need to be sequenced relative to each other.  Validators refuse to sign transactions that do not meet this requirement and packages that include entry functions that accept a `Clock` or `&mut Clock` fail to publish.

The following example shows how to test 'Clock'-dependent code by manually incrementing its timestamp, which is possible only in test code: 

```
module sui::clock {
    #[test_only]
    public fun increment_for_testing(clock: &mut Clock, tick: u64);
}
```



## Epoch timestamps

You can use the following function to access the timestamp for the start of the current epoch for all transactions (including ones that do not go through consensus):

```
module sui::tx_context {
    public fun epoch_timestamp_ms(ctx: &TxContext): u64;
}
```

The preceding example returns the point in time when the current epoch started, as a millisecond granularity unix timestamp in a `u64`.  **This value changes roughly once every 24 hours**, when the epoch changes.

Tests based on `sui::test_scenario` for time-sensitive code that uses the previous function can use `sui::test_scenario::later_epoch` to simulate the progress of time:

```
module sui::test_scenario {
    public fun later_epoch(
        scenario: &mut Scenario,
        delta_ms: u64,
        sender: address,
    ): TransactionEffects;
}
```

The preceding code behaves like `sui::test_scenario::next_epoch` (finishes the current transaction and epoch), but also increments the timestamp by `delta_ms` milliseconds.
