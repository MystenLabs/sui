// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses P0=0x0 --simulator

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

//# run-graphql
# Querying abilities for concrete types

{
    token: type(type: "0x2::token::Token<0x2::sui::SUI>") {
        abilities
    }

    coin: type(type: "0x2::coin::Coin<0x2::sui::SUI>") {
        abilities
    }

    balance: type(type: "0x2::balance::Balance<0x2::sui::SUI>") {
        abilities
    }

    coin_vector: type(type: "vector<0x2::coin::Coin<0x2::sui::SUI>>") {
        abilities
    }

    prim_vector: type(type: "vector<u64>") {
        abilities
    }
}

//# run-graphql
# Unhappy path, type arguments too deep.

{
    type(type: """
        vector<vector<vector<vector<
        vector<vector<vector<vector<
        vector<vector<vector<vector<
        vector<vector<vector<vector<
            vector<u8>
        >>>>
        >>>>
        >>>>
        >>>>
        """) {
            abilities
        }
}

//# publish
module P0::m {
    struct S0<T> {
        xs: vector<vector<vector<vector<
            vector<vector<vector<vector<
                T
            >>>>
            >>>>
    }

    struct S1<T> {
        xss: S0<S0<S0<S0<S0<S0<S0<S0<
             S0<S0<S0<S0<S0<S0<S0<S0<
                 T
             >>>>>>>>
             >>>>>>>>
    }
}

//# create-checkpoint

//# run-graphql

# Unhappy path, value nesting too deep.

{
    type(type: "@{P0}::m::S1<u32>") {
        layout
    }
}
