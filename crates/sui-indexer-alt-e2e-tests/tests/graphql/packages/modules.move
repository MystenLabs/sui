// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

//# publish --upgradeable --sender A
module P::A { public fun f(): u64 { 42 } }
module P::B { public fun f(): u64 { 42 } }
module P::C { public fun f(): u64 { 42 } }
module P::D { public fun f(): u64 { 42 } }
module P::E { public fun f(): u64 { 42 } }
module P::F { public fun f(): u64 { 42 } }
module P::G { public fun f(): u64 { 42 } }
module P::H { public fun f(): u64 { 42 } }
module P::I { public fun f(): u64 { 42 } }
module P::J { public fun f(): u64 { 42 } }
module P::K { public fun f(): u64 { 42 } }
module P::L { public fun f(): u64 { 42 } }
module P::M { public fun f(): u64 { 42 } }
module P::N { public fun f(): u64 { 42 } }
module P::O { public fun f(): u64 { 42 } }
module P::P { public fun f(): u64 { 42 } }
module P::Q { public fun f(): u64 { 42 } }
module P::R { public fun f(): u64 { 42 } }
module P::S { public fun f(): u64 { 42 } }
module P::T { public fun f(): u64 { 42 } }
module P::U { public fun f(): u64 { 42 } }
module P::V { public fun f(): u64 { 42 } }
module P::W { public fun f(): u64 { 42 } }
module P::X { public fun f(): u64 { 42 } }
module P::Y { public fun f(): u64 { 42 } }
module P::Z { public fun f(): u64 { 42 } }

//# create-checkpoint

//# run-graphql --cursors "J" "P"
{
  package(address: "@{P}") {
    all: modules(first: 26) { ...M }
    first: modules(first: 3) { ...M }
    last: modules(last: 3) { ...M }

    firstBefore: modules(first: 3, before: "@{cursor_1}") { ...M }
    lastAfter: modules(last: 3, after: "@{cursor_0}") { ...M }

    firstAfter: modules(first: 3, after: "@{cursor_0}") { ...M }
    lastBefore: modules(last: 3, before: "@{cursor_1}") { ...M }

    afterBefore: modules(after: "@{cursor_0}", before: "@{cursor_1}") { ...M }
  }
}

fragment M on MoveModuleConnection {
  pageInfo {
    hasPreviousPage
    hasNextPage
  }
  nodes {
    name
  }
}
