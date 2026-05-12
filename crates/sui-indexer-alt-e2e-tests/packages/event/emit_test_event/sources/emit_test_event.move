// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module emit_test_event::emit_test_event;

use sui::event;

public struct TestEvent has copy, drop {
	value: u64,
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
