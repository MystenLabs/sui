// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A publish with `init` pins the published package's dependencies `exact` (init runs in-tx), so init
// observes exactly the dependency versions the package was published against -- not the latest. Each
// init package's `init` stores `Dep::val()` (1 in v1, 2 in v2) into a shared `Config`, so the
// observed `v` tells us which version of `Dep` init ran against. A fresh `//# publish` can only
// depend on the root version (v1); to publish against the upgraded v2 we use a PTB `Publish` whose
// dependency list overrides `Dep` to v2.
// The PTB-publish cases also pair the publish (which pins `Dep` exact to its declared version) with
// another command whose transitive `Dep` is pinned `at_least` by a public consumer: a transitive
// version below the publish's `Dep` unifies up to it (succeeds), while one above it conflicts.

//# init --addresses Dep_V1=0x0 Dep_V2=0x0 ConsumerV1=0x0 ConsumerV2=0x0 PInitV1=0x0 PInitOld=0x0 PPtb=0x0 --accounts A

//# publish --upgradeable --sender A
module Dep_V1::d;

public fun val(): u64 { 1 }

public fun ping() {}

//# publish --upgradeable --dependencies Dep_V1 --sender A
module ConsumerV1::c;
use Dep_V1::d;

// Public consumer: calling `consume` pins `Dep` `at_least` the version this package depends on.
public fun consume() { d::ping() }

//# publish --dependencies Dep_V1 --sender A
module PInitV1::m;
use Dep_V1::d;

public struct Config has key { id: sui::object::UID, v: u64 }

// Published while only Dep v1 exists => init runs against v1 => v = 1.
fun init(ctx: &mut sui::tx_context::TxContext) {
    sui::transfer::share_object(Config { id: sui::object::new(ctx), v: d::val() })
}

//# view-object 3,1

//# upgrade --package Dep_V1 --upgrade-capability 1,1 --sender A
module Dep_V2::d;

public fun val(): u64 { 2 }

public fun ping() {}

//# upgrade --package ConsumerV1 --upgrade-capability 2,1 --dependencies Dep_V2 --sender A
module ConsumerV2::c;
use Dep_V2::d;

// v2 of the consumer depends on Dep v2 => calling `consume` pins `Dep` `at_least` 2.
public fun consume() { d::ping() }

//# publish --dependencies Dep_V1 --sender A
module PInitOld::m;
use Dep_V1::d;

public struct Config has key { id: sui::object::UID, v: u64 }

// Published against Dep v1 even though v2 now exists => init still runs against v1 => v = 1.
fun init(ctx: &mut sui::tx_context::TxContext) {
    sui::transfer::share_object(Config { id: sui::object::new(ctx), v: d::val() })
}

//# view-object 7,1

//# stage-package
module PPtb::m;
use Dep_V1::d;

public struct Config has key { id: sui::object::UID, v: u64 }

fun init(ctx: &mut sui::tx_context::TxContext) {
    sui::transfer::share_object(Config { id: sui::object::new(ctx), v: d::val() })
}

//# programmable --sender A --inputs @A
// PTB publish with `init`, dep set to v2 => init runs against v2 => v = 2.
//> 0: Publish(PPtb, [Dep_V2, sui, std]);
//> 1: TransferObjects([Result(0)], Input(0));

//# view-object 10,1

//# programmable --sender A --inputs @A
// init pins Dep exact(1); `ping` pins Dep exact(2) => exact/exact conflict => InvalidLinkage.
//> 0: Publish(PPtb, [Dep_V1, sui, std]);
//> 1: Dep_V2::d::ping();
//> 2: TransferObjects([Result(0)], Input(0));

//# programmable --sender A --inputs @A
// init pins Dep exact(2); ConsumerV1 pins Dep at_least(1) (transitive, lower) => unifies to exact(2)
// => init runs v2 => v = 2.
//> 0: Publish(PPtb, [Dep_V2, sui, std]);
//> 1: ConsumerV1::c::consume();
//> 2: TransferObjects([Result(0)], Input(0));

//# view-object 13,1

//# programmable --sender A --inputs @A
// init pins Dep exact(1); ConsumerV2 pins Dep at_least(2) (transitive, higher) => exact(1) < 2 =>
// InvalidLinkage.
//> 0: Publish(PPtb, [Dep_V1, sui, std]);
//> 1: ConsumerV2::c::consume();
//> 2: TransferObjects([Result(0)], Input(0));
