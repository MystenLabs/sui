// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Exercise test functions that emit authenticated events

//# init --addresses test=0x0 --accounts A B --simulator --enable-authenticated-event-streams

//# publish

module test::event_streams {
	use sui::event;

	public struct AuthEvent has copy, drop {
		value: u64,
	}

	// test that reading o2 and updating o1 works
	public entry fun emit_event(value: u64) {
		// emit an event so the world can see the new value
		let ev = AuthEvent { value };
		event::emit_authenticated(ev);
	}
}

//# run test::event_streams::emit_event --sender A --args 10

//# create-checkpoint

//# view-object 2,0

// Checkpoint 2: Add second event - should trigger MMR merge at height 1
//# run test::event_streams::emit_event --sender A --args 20

//# create-checkpoint

//# view-object 2,0

// Checkpoint 3: Add third event - should place at MMR[0]
//# run test::event_streams::emit_event --sender A --args 30

//# create-checkpoint

//# view-object 2,0

// Checkpoint 4: Add fourth event - should trigger cascade merge to MMR[2]
//# run test::event_streams::emit_event --sender A --args 40

//# create-checkpoint

//# view-object 2,0

// Run the test with:
// $ cargo nextest run -p sui-adapter-transactional-tests emit_authenticated
