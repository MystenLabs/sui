// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Regression test for iterative bloom scanning with compound bloom keys.
// Two modules in the same package: Common (high-activity) and Rare (low-activity).
// Without compound bloom keys, querying for Rare::Signal matches every checkpoint with
// ANY activity from package P. With compound keys the bloom also encodes the module name,
// so the scanner can reject Common-only checkpoints at the bloom level.
// The noise checkpoints also exercise adaptive batch sizing -- the scanner must continue
// past false-positive batches across multiple empty bloom blocks to find real matches.

//# init --protocol-version 70 --addresses P=0x0 --accounts A --simulator

//# publish
module P::Common {
    use sui::event;

    public struct Ping has copy, drop { value: u64 }

    public entry fun ping(value: u64) {
        event::emit(Ping { value });
    }
}

module P::Rare {
    use sui::event;

    public struct Signal has copy, drop { value: u64 }

    public entry fun signal(value: u64) {
        event::emit(Signal { value });
    }
}

// Block 0: Signal at cp 1, then Ping noise at cps 2-11
//# run P::Rare::signal --sender A --args 1

//# create-checkpoint

//# run P::Common::ping --sender A --args 1

//# create-checkpoint

//# run P::Common::ping --sender A --args 2

//# create-checkpoint

//# run P::Common::ping --sender A --args 3

//# create-checkpoint

//# run P::Common::ping --sender A --args 4

//# create-checkpoint

//# run P::Common::ping --sender A --args 5

//# create-checkpoint

//# run P::Common::ping --sender A --args 6

//# create-checkpoint

//# run P::Common::ping --sender A --args 7

//# create-checkpoint

//# run P::Common::ping --sender A --args 8

//# create-checkpoint

//# run P::Common::ping --sender A --args 9

//# create-checkpoint

//# run P::Common::ping --sender A --args 10

//# create-checkpoint

// Skip to block 3
//# create-checkpoint 2989

// Block 3: second Signal at cp 3001
//# run P::Rare::signal --sender A --args 2

//# create-checkpoint

//# run-graphql
# Query by type: bloom matches package P in blocks 0 and 3, but compound keys let the
# scanner reject the 10 Common::Ping checkpoints at the bloom level.
# Query by module: tests module-level bloom selectivity directly.
{
  byType: scanEvents(filter: { type: "@{P}::Rare::Signal", beforeCheckpoint: 3002 }) { ...EV }
  byModule: scanEvents(filter: { module: "@{P}::Rare", beforeCheckpoint: 3002 }) { ...EV }
  backward: scanEvents(last: 10, filter: { type: "@{P}::Rare::Signal", beforeCheckpoint: 3002 }) { ...EV }
}

fragment EV on EventConnection {
  pageInfo { startCursor endCursor hasPreviousPage hasNextPage }
  edges { cursor node { sequenceNumber sender { address } contents { type { repr } } } }
}

//# run-graphql --cursors {"c":1,"t":2,"e":0}
# Paginate forward after the first Signal — must cross 10 noise checkpoints in block 0,
# then 2 empty blocks, to find the second Signal in block 3.
{
  afterFirst: scanEvents(first: 10, after: "@{cursor_0}", filter: { type: "@{P}::Rare::Signal", beforeCheckpoint: 3002 }) { ...EV }
}

fragment EV on EventConnection {
  pageInfo { startCursor endCursor hasPreviousPage hasNextPage }
  edges { cursor node { sequenceNumber sender { address } contents { type { repr } } } }
}
