// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module emit_test_event::emit_test_event;

use sui::clock::Clock;
use sui::event;

public struct TestEvent has copy, drop {
	value: u64,
}

/// Event that carries an address payload. Used by `asTransactionObject` tests so they can
/// extract the address out of the event and resolve it via the parent transaction.
public struct TestAddressEvent has copy, drop {
	address_event_id: address,
}

public struct TestObject has key, store {
	id: UID,
	value: u64,
}

public fun emit_test_event() {
	event::emit(TestEvent {
		value: 1,
	});
}

public fun emit_with_value(value: u64) {
	event::emit(TestEvent { value })
}

/// Emit a `TestEvent` and create a `TestObject` in the same transaction. Used to
/// verify chained `event { transaction { effects { objectChanges } } }` queries.
public fun emit_and_create(value: u64, ctx: &mut TxContext): TestObject {
	event::emit(TestEvent { value });
	TestObject {
		id: object::new(ctx),
		value,
	}
}

/// Create a `TestObject` without emitting an event. Used as setup for tests that need a
/// pre-existing object to mutate later.
public fun create_object(value: u64, ctx: &mut TxContext): TestObject {
	TestObject {
		id: object::new(ctx),
		value,
	}
}

/// Mutate an existing `TestObject` and emit a `TestAddressEvent` whose `address_event_id`
/// is the mutated object's address. Lets `asTransactionObject` tests show both the
/// `inputState` (pre-mutation) and `outputState` (post-mutation).
public fun mutate_and_emit(obj: &mut TestObject, new_value: u64) {
	obj.value = new_value;
	event::emit(TestAddressEvent {
		address_event_id: object::id(obj).to_address(),
	});
}

/// Emit a `TestAddressEvent` whose `address_event_id` is the (read-only) clock's address.
/// Used by `asTransactionObject` tests for the `ConsensusObjectRead` variant.
public fun emit_with_clock(clock: &Clock) {
	event::emit(TestAddressEvent {
		address_event_id: object::id(clock).to_address(),
	});
}
