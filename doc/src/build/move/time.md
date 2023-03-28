---
title: Accessing Time in Sui Move
---

You have options when needing to access network-based time for your transactions. If you need a near real-time measurement (within a few seconds), use the immutable reference of time provided by the `Clock` module in Sui Move. The reference value from this module updates with every network checkpoint. If you don't need as current a time slice, use the `epoch_timestamp_ms` function to capture the precise moment the current epoch started.

## The sui::clock::Clock module

To access a prompt timestamp, you must pass a read-only reference of `sui::clock::Clock` as an entry function parameter in your transactions. An instance of `Clock` is provided at address `0x6`, no new instances can be created.

Extract a unix timestamp in milliseconds from an instance of `Clock` using

```
module sui::clock {
    public fun timestamp_ms(clock: &Clock): u64;
}
```

The example below demonstrates an entry function that emits an event containing a timestamp from the `Clock`:

```
module example::clock {
    use sui::clock::{Self, Clock};
    use sui::event;

    struct TimeEvent has copy, drop, store { 
        timestamp_ms: u64
    }

    entry fun access(clock: &Clock) {
        event::emit(TimeEvent {
            timestamp_ms: clock::timestamp_ms(clock),
        });
    }
}
```

A call to the previous entry function takes the following form, passing `0x6` as the address for the `Clock` parameter:

```
sui client call --package <EXAMPLE> --module 'clock' --function 'access' --args '0x6' --gas-budget 10000
```

**Expect the `Clock` timestamp to change every 2 to 3 seconds**, at the rate the network commits checkpoints.

Successive calls to `sui::clock::timestamp_ms` in the same transaction always produce the same result (transactions are considered to take effect instantly), but timestamps from `Clock` are otherwise monotonic across transactions that touch the same shared objects:  Successive transactions seeing a greater or equal timestamp to their predecessors.

Any transaction that requires access to a `Clock` must go through [consensus](/learn/architecture/consensus) because the only available instance is a [shared object](/learn/objects#shared). As a result, this technique is not suitable for transactions that must use the single-owner fast-path (see [Epoch timestamps](#epoch-timestamps) for a single-owner-compatible source of timestamps).

**Transactions that use the clock must accept it as an immutable reference** (not a mutable reference or value).  This prevents contention, as transactions that access the `Clock` can only read it, so do not need to be sequenced relative to each other.  Validators refuse to sign transactions that do not meet this requirement and packages that include entry functions that accept a `Clock` or `&mut Clock` fail to publish.

The following functions test 'Clock'-dependent code by manually creating (and sharing) a `Clock` object and incrementing its timestamp. This is possible only in test code: 

```
module sui::clock {
    #[test_only]
    public fun create_for_testing(ctx: &mut TxContext);

    #[test_only]
    public fun increment_for_testing(clock: &mut Clock, tick: u64);
}
```

The next example presents a simple test that creates a `Clock`, increments it, and then checks its value:

```
module example::clock_tests {
    use sui::clock::{Self, Clock};
    use sui::test_scenario as ts;

    #[test]
    fun creating_a_clock_and_incrementing_it() {
        let ts = ts::begin(@0x1);
        let ctx = ts::ctx(&mut ts);

        clock::create_for_testing(ctx);

        let clock = ts::take_shared<Clock>(&ts);
        clock::increment_for_testing(&mut clock, 20);
        clock::increment_for_testing(&mut clock, 22);
        assert!(clock::timestamp_ms(&clock) == 42, 0);

        ts::return_shared(clock);
        ts::end(ts);
    }
}

```

## Epoch timestamps

You can use the following function to access the timestamp for the start of the current epoch for all transactions (including ones that do not go through consensus):

```
module sui::tx_context {
    public fun epoch_timestamp_ms(ctx: &TxContext): u64;
}
```

The preceding function returns the point in time when the current epoch started, as a millisecond granularity unix timestamp in a `u64`.  **This value changes roughly once every 24 hours**, when the epoch changes.

Tests based on `sui::test_scenario` can use `later_epoch` (following code), to exercise time-sensitive code that uses `epoch_timestamp_ms` (previous code):

```
module sui::test_scenario {
    public fun later_epoch(
        scenario: &mut Scenario,
        delta_ms: u64,
        sender: address,
    ): TransactionEffects;
}
```

`later_epoch` behaves like `sui::test_scenario::next_epoch` (finishes the current transaction and epoch in the test scenario), but also increments the timestamp by `delta_ms` milliseconds to simulate the progress of time.
