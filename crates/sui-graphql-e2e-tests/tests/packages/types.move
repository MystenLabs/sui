// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --simulator

//# run-graphql
# Happy path -- valid type, get everything
{
    type(type: "0x2::priority_queue::PriorityQueue<0x2::coin::Coin<0x2::sui::SUI>>") {
        repr
        signature
        layout
    }
}

//# run-graphql
# Happy path -- primitive type
{
    type(type: "u64") {
        repr
        signature
        layout
    }
}

//# run-graphql
# Happy path -- primitive type with generic parameter
{
    type(type: "vector<u64>") {
        repr
        signature
        layout
    }
}

//# run-graphql
# Unhappy path -- bad type tag (failed to parse)
{
    type(type: "not_a_type") {
        repr
        signature
        layout
    }
}

//# run-graphql

# Semi-happy path -- the input looks like a type, but that type
# doesn't exist. Depending on which fields you ask for, this request
# may still succeed.

{
    type(type: "0x42::not::Here") {
        repr
        signature
    }
}

//# run-graphql

# Unhappy side of semi-happy path -- asking for a layout for a type
# that doesn't exist won't work.
#
# TODO: This currently produces an INTERNAL_SERVER_ERROR, but should
# produce a user error. This because, like other parts of our
# codebase, we don't have enough differentiation in error types for
# MoveType to signal an error in layout calculation and the GraphQL
# field implementations to know that it is a user error or an internal
# error.
{
    type(type: "0x42::not::Here") {
        layout
    }
}
