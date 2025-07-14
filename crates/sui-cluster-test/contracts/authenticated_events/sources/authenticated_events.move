// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module authenticated_events::authenticated_events;

use sui::event;
use sui::event::EventStreamHead;
use sui::accumulator;
use sui::tx_context::{Self, TxContext};
use sui::event::EventStreamCap;

// A type defined within this module to get the capability for the default stream.
public struct AuthenticatedEventsWitness has copy, drop {}

public struct AuthenticatedEvent has copy, drop { 
    value: u64,
}


public fun emit_to_default_stream(value: u64, ctx: &mut TxContext) {
    let cap = event::default_event_stream_cap<AuthenticatedEventsWitness>(ctx);
    event::emit_authenticated(&cap, AuthenticatedEvent { value });
    event::destroy_cap(cap);
}