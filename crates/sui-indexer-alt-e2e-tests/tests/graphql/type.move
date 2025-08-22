// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --addresses P0=0x0 P1=0x0 --simulator --accounts A

//# run-graphql
{ # Happy paths
  complex: type(type: "0x2::priority_queue::PriorityQueue<0x2::coin::Coin<0x2::sui::SUI>>") { ...Type }
  coin: type(type: "0x2::coin::Coin<0x2::sui::SUI>") { ...Type }
  token: type(type: "0x2::token::Token<0x2::sui::SUI>") { ...Type }
  balance: type(type: "0x2::balance::Balance<0x2::sui::SUI>") { ...Type }
  primitive: type(type: "u64") { ...Type }
  prim_vector: type(type: "vector<u64>") { ...Type }
  coin_vector: type(type: "vector<0x2::coin::Coin<0x2::sui::SUI>>") { ...Type }
}

fragment Type on MoveType {
  repr
  abilities
  signature
  layout
}

//# run-graphql
{ # Multi-get
  multiGetTypes(keys: [
    "0x2::priority_queue::PriorityQueue<0x2::coin::Coin<0x2::sui::SUI>>",
    "0x2::coin::Coin<0x2::sui::SUI>",
    "0x2::token::Token<0x2::sui::SUI>",
    "0x2::balance::Balance<0x2::sui::SUI>",
    "u64",
    "vector<u64>",
    "vector<0x2::coin::Coin<0x2::sui::SUI>>",
    "0x2::coin::Coin<0x2::doesnt::EXIST>",
  ]) {
    repr
  }
}

//# run-graphql
{ # Unhappy path -- parse failure
  type(type: "not_a_type") { repr }
}

//# run-graphql
{ # Semi-happy path -- type parses but doesn't exist
  # Request still succeeds, because the type parses.
  type(type: "0x42::not::Here") {
    repr
    signature
  }
}

//# run-graphql
{ # Unhappy side of semi-happy path -- type parses but doesn't exist
  type(type: "0x42::not::Here") {
    layout
  }
}

//# run-graphql
{ # Unhappy path, type arguments too deep.
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

//# run-graphql
{ # Unhappy path, type argument arity mismatch
  type(type: "0x2::coin::Coin<0x2::sui::SUI, u64>") {
    abilities
  }
}

//# publish --upgradeable --sender A
module P0::m {
  public struct S0<T> {
    xs: vector<vector<vector<vector<
        vector<vector<vector<vector<
            T
        >>>>
        >>>>
  }

  public struct S1<T> {
    xss: S0<S0<S0<S0<S0<S0<S0<S0<
         S0<S0<S0<S0<S0<S0<S0<S0<
             T
         >>>>>>>>
         >>>>>>>>
  }
}

//# create-checkpoint

//# run-graphql
{ # Unhappy path, value nesting too deep.
    type(type: "@{P0}::m::S1<u32>") {
        layout
    }
}

//# upgrade --package P0 --upgrade-capability 8,1 --sender A
module P0::m {
  public struct S0<T> {
    xs: vector<vector<vector<vector<
        vector<vector<vector<vector<
            T
        >>>>
        >>>>
  }

  public struct S1<T> {
    xss: S0<S0<S0<S0<S0<S0<S0<S0<
         S0<S0<S0<S0<S0<S0<S0<S0<
             T
         >>>>>>>>
         >>>>>>>>
  }
}

//# create-checkpoint

//# run-graphql
{ # Canonicalizing relocates package IDs
  atP0: type(type: "@{obj_8_0}::m::S0<u32>") {
    repr
  }

  atP1: type(type: "@{obj_11_0}::m::S0<u32>") {
    repr
  }
}
