// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// APIs for accessing time from move calls, via the `Clock`: a unique
/// shared object that is created at 0x6 during genesis.
module sui::clock;

/// Sender is not @0x0 the system address.
const ENotSystemAddress: u64 = 0;

#[test_only]
/// The test Clock was set to a timestamp earlier than its current one.
const ETimestampDecreased: u64 = 1;

/// Singleton shared object that exposes time to Move calls.  This
/// object is found at address 0x6, and can only be read (accessed
/// via an immutable reference) by entry functions.
///
/// Entry Functions that attempt to accept `Clock` by mutable
/// reference or value will fail to verify, and honest validators
/// will not sign or execute transactions that use `Clock` as an
/// input parameter, unless it is passed by immutable reference.
public struct Clock has key {
    id: UID,
    /// The clock's timestamp, which is set automatically by a
    /// system transaction every time consensus commits a
    /// schedule, or by `sui::clock::increment_for_testing` during
    /// testing.
    timestamp_ms: u64,
}

/// The `clock`'s current timestamp as a running total of
/// milliseconds since an arbitrary point in the past.
public fun timestamp_ms(clock: &Clock): u64 {
    clock.timestamp_ms
}

/// A reference to the `Clock` without it being a transaction input.
/// It is the same `Clock`, at the same timestamp, as one passed into
/// the transaction.
///
/// New code should read the time this way rather than take a `Clock`
/// parameter, so that callers do not change when a function starts to
/// need the time. Existing functions with `&Clock` arguments can be
/// supplied one from anywhere via `borrow()`.
public fun borrow(): &Clock {
    native_borrow()
}
native fun native_borrow(): &Clock;

#[allow(unused_function)]
/// Create and share the singleton Clock -- this function is
/// called exactly once, during genesis.
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    transfer::share_object(Clock {
        id: object::clock(),
        // Initialised to zero, but set to a real timestamp by a
        // system transaction before it can be witnessed by a move
        // call.
        timestamp_ms: 0,
    })
}

#[allow(unused_function)]
fun consensus_commit_prologue(clock: &mut Clock, timestamp_ms: u64, ctx: &TxContext) {
    // Validator will make a special system call with sender set as 0x0.
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    clock.timestamp_ms = timestamp_ms
}

#[
    test_only,
    deprecated(
        note = b"Use `sui::clock::borrow` for the Clock and `sui::test_scenario::set_clock_timestamp` or `sui::test_scenario::increment_clock_timestamp` to change its time",
    ),
]
/// Expose the functionality of `create()` (usually only done during
/// genesis) for tests that want to create a Clock.
public fun create_for_testing(ctx: &mut TxContext): Clock {
    Clock {
        id: object::new(ctx),
        timestamp_ms: borrow().timestamp_ms(),
    }
}

#[
    test_only,
    deprecated(
        note = b"Use `sui::clock::borrow` for the Clock and `sui::test_scenario::set_clock_timestamp` or `sui::test_scenario::increment_clock_timestamp` to change its time",
    ),
]
/// For transactional tests (if a Clock is used as a shared object).
public fun share_for_testing(clock: Clock) {
    transfer::share_object(clock)
}

#[test_only, deprecated(note = b"Use `sui::test_scenario::increment_clock_timestamp`")]
public fun increment_for_testing(clock: &mut Clock, tick: u64) {
    set_for_testing(clock, borrow().timestamp_ms() + tick);
}

#[test_only, deprecated(note = b"Use `sui::test_scenario::set_clock_timestamp`")]
public fun set_for_testing(clock: &mut Clock, timestamp_ms: u64) {
    set_timestamp_ms_for_testing(timestamp_ms);
    clock.timestamp_ms = timestamp_ms;
}

#[
    test_only,
    deprecated(
        note = b"Use `sui::clock::borrow` for the Clock and `sui::test_scenario::set_clock_timestamp` or `sui::test_scenario::increment_clock_timestamp` to change its time",
    ),
]
public fun destroy_for_testing(clock: Clock) {
    let Clock { id, timestamp_ms: _ } = clock;
    id.delete();
}

#[test_only]
/// Sets the timestamp of the `Clock` returned by `borrow`, including
/// references already borrowed, since tests have no consensus commits
/// to set it.
/// Aborts with `ETimestampDecreased` if `timestamp_ms` is earlier than
/// the current timestamp.
public(package) fun set_timestamp_ms_for_testing(timestamp_ms: u64) {
    assert!(timestamp_ms >= borrow().timestamp_ms(), ETimestampDecreased);
    native_set_timestamp_ms_for_testing(timestamp_ms);
}

#[test_only]
native fun native_set_timestamp_ms_for_testing(timestamp_ms: u64);
