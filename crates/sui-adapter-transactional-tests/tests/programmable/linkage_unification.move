// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Valid unifications resolve (no InvalidLinkage): exact ~ at_least when exact >= at_least, the
// exact == at_least boundary, and at_least ~ at_least taking the max.

//# init --addresses Test=0x0 DepV1=0x0 DepV2=0x0 DepConsumerV1=0x0 DepConsumerV2=0x0 OtherConsumer=0x0 --accounts A

//# publish --upgradeable --sender A
module DepV1::M1;
public fun f1() { }

//# upgrade --package DepV1 --upgrade-capability 1,1 --sender A
module DepV2::M1;
public fun f1() { }
public fun f2() { }

//# publish --upgradeable --dependencies DepV1 --sender A
// Public consumer; v1 pins Dep at_least 1, v2 pins Dep at_least 2.
module DepConsumerV1::M;
public fun consume() { DepV1::M1::f1() }

//# upgrade --package DepConsumerV1 --upgrade-capability 3,1 --dependencies DepV2 --sender A
module DepConsumerV2::M;
public fun consume() { DepV2::M1::f2() }

//# publish --dependencies DepV1 --sender A
// Distinct package pinning Dep at_least 1 (the at_least/at_least case needs two different packages;
// two versions of one package would conflict on that package itself).
module OtherConsumer::M;
public fun consume() { DepV1::M1::f1() }

//# stage-package
module Test::M;
fun init(_ctx: &mut TxContext) { }

//# programmable --sender A --inputs @A
// exact(DepV2 v2) ~ at_least(Dep v1) => exact(v2).
//> 0: Publish(Test, [DepV2, sui, std]);
//> 1: OtherConsumer::M::consume();
//> TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
// boundary: exact(DepV2 v2) ~ at_least(Dep v2).
//> 0: Publish(Test, [DepV2, sui, std]);
//> 1: DepConsumerV2::M::consume();
//> TransferObjects([Result(0)], Input(0))

//# programmable --sender A
// at_least(Dep v1) ~ at_least(Dep v2) => at_least(v2).
//> 0: OtherConsumer::M::consume();
//> 1: DepConsumerV2::M::consume();
