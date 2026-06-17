// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Reading the consensus commit timestamp via the `tx_context` native (without taking the `Clock`
// as an input) returns the current `Clock` time, which is driven here by `advance-clock`.

//# init --accounts A --addresses test=0x0 --simulator

//# publish
module test::ccts {
    use sui::event;

    public struct TimeEvent has copy, drop, store {
        timestamp_ms: u64,
    }

    public entry fun emit_consensus_commit_timestamp(ctx: &TxContext) {
        event::emit(TimeEvent { timestamp_ms: ctx.timestamp_ms() });
    }
}

//# advance-clock --duration-ns 42000000

//# run test::ccts::emit_consensus_commit_timestamp --sender A
