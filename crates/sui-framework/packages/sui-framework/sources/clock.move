// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// APIs for accessing time from move calls, via the `Clock`: a unique
/// shared object that is created at 0x6 during genesis.
module sui::clock;

/// Sender is not @0x0 the system address.
const ENotSystemAddress: u64 = 0;

#[test_only]
/// The test already holds a `Clock`.
const EClockAlreadyExists: u64 = 1;

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

#[test_only]
/// Expose the functionality of `create()` (usually only done during
/// genesis) for tests that want to create a Clock. A test holds at
/// most one `Clock` at a time, so that the `Clock` returned by
/// `borrow` cannot disagree with it; it has the same id as the
/// genesis `Clock`.
public fun create_for_testing(_ctx: &mut TxContext): Clock {
    assert!(!native_exists_for_testing(), EClockAlreadyExists);
    native_set_timestamp_ms_for_testing(0);
    Clock {
        id: object::clock(),
        timestamp_ms: 0,
    }
}

#[test_only]
/// For transactional tests (if a Clock is used as a shared object).
public fun share_for_testing(clock: Clock) {
    transfer::share_object(clock)
}

#[test_only]
public fun increment_for_testing(clock: &mut Clock, tick: u64) {
    clock.timestamp_ms = clock.timestamp_ms + tick;
    native_set_timestamp_ms_for_testing(clock.timestamp_ms);
}

#[test_only]
public fun set_for_testing(clock: &mut Clock, timestamp_ms: u64) {
    assert!(timestamp_ms >= clock.timestamp_ms);
    clock.timestamp_ms = timestamp_ms;
    native_set_timestamp_ms_for_testing(clock.timestamp_ms);
}

#[test_only]
public fun destroy_for_testing(clock: Clock) {
    let Clock { id, timestamp_ms: _ } = clock;
    id.delete();
    native_clear_for_testing();
}

#[test_only]
/// Keeps the `Clock` returned by `borrow` in step with the `Clock` a
/// test creates or mutates, since tests have no consensus commits to
/// set it.
native fun native_set_timestamp_ms_for_testing(timestamp_ms: u64);

#[test_only]
/// Whether the test holds a `Clock`.
native fun native_exists_for_testing(): bool;

#[test_only]
/// Forgets the test's `Clock`, so that `borrow` returns the genesis
/// `Clock` again and the test can create another.
native fun native_clear_for_testing();
