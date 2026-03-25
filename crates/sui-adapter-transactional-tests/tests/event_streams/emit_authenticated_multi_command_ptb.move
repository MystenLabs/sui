// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Verify authenticated events used in multi-command ptbs.

//# init --addresses test=0x0 --accounts A --enable-authenticated-event-streams

//# publish

module test::events {
	use sui::event;

	public struct RegularEvent has copy, drop {
		value: u64,
	}

	public struct AuthEvent has copy, drop {
		value: u64,
	}

	public entry fun emit_regular(value: u64) {
		event::emit(RegularEvent { value });
	}

	public entry fun emit_regular_multiple(start: u64, count: u64) {
		let mut i = 0;
		while (i < count) {
			event::emit(RegularEvent { value: start + i });
			i = i + 1;
		};
	}

	public entry fun emit_auth(value: u64) {
		event::emit_authenticated(AuthEvent { value });
	}
}

//# run test::events::emit_auth --sender A --args 42
// Single-command PTB with authenticated event (baseline). Expected event_indices:[0].

//# programmable --sender A --inputs 0u64 3u64 999u64
// Multi-command PTB: 3 regular events then 1 authenticated event.
// Expected event_indices:[3] (the authenticated event is the 4th event globally).
//> 0: test::events::emit_regular_multiple(Input(0), Input(1));
//> 1: test::events::emit_auth(Input(2));

//# programmable --sender A --inputs 0u64 5u64 100u64 10u64 2u64 200u64
// Multi-command PTB with interleaved regular and authenticated events.
// Command 0: 5 regular events (global indices 0-4)
// Command 1: 1 authenticated event (global index 5)
// Command 2: 2 regular events (global indices 6-7)
// Command 3: 1 authenticated event (global index 8)
// Expected event_indices:[5, 8].
//> 0: test::events::emit_regular_multiple(Input(0), Input(1));
//> 1: test::events::emit_auth(Input(2));
//> 2: test::events::emit_regular_multiple(Input(3), Input(4));
//> 3: test::events::emit_auth(Input(5));

//# create-checkpoint

//# view-object 2,0 --hide-contents
