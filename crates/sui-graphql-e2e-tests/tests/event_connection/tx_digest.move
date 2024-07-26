// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that fetching events filtered on a tx digest that has no events correctly returns no nodes.
// Also tests that fetching events filtered on a tx digest that has events returns the correct
// number of page-limit-bound nodes.

//# init --protocol-version 48 --addresses Test=0x0 --accounts A --simulator

//# publish
module Test::M1 {
    use sui::event;

    public struct EventA has copy, drop {
        new_value: u64
    }

    public entry fun no_emit(value: u64): u64 {
        value
    }

    public entry fun emit_2(value: u64) {
        event::emit(EventA { new_value: value });
        event::emit(EventA { new_value: value + 1})
    }
}

//# run Test::M1::no_emit --sender A --args 0

//# run Test::M1::emit_2 --sender A --args 2

//# create-checkpoint

//# run-graphql
{
    transactionBlocks {
        nodes {
            digest
        }
    }
}

//# run-graphql
{
    events(filter: {transactionDigest: "5qVugAqZ6ANMTadHevvJaUyU2c65paKm1n6UGCJHTAbD"}) {
        nodes {
            json
        }
    }
}

//# run-graphql
{
    events(filter: {transactionDigest: "Ar4ascrErFfQAEPEcNxhMfJok8FvkhSocVjPBm9vUQa2"}) {
        nodes {
            json
        }
    }
}
