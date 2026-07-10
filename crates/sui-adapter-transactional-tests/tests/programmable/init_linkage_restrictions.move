// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Publish with `init` pins the published package's deps `exact` (init runs in-tx). A conflicting
// `exact`/`at_least` constraint from a root call then yields InvalidLinkage. Publish without `init`
// contributes nothing.

//# init --addresses Test=0x0 NoInit=0x0 DepV1=0x0 DepV2=0x0 V1Consumer=0x0 DepConsumerV1=0x0 DepConsumerV2=0x0 --accounts A

//# publish --upgradeable --sender A
module DepV1::M1;
fun init(_ctx: &mut TxContext) { }
public fun f1() { }

//# upgrade --package DepV1 --upgrade-capability 1,1 --sender A
module DepV2::M1;
fun init(_ctx: &mut TxContext) { }
public fun f1() { }
public fun f2() { }

//# stage-package
module Test::M;
fun init(_ctx: &mut TxContext) { }

//# stage-package
module NoInit::M;
public fun f() { }

//# publish --dependencies DepV1 --sender A
// Depends on DepV1, reached via a private entry fn => deps pinned exact => DepV1 (v1) exact.
module V1Consumer::M;
entry fun consume_v1() { DepV1::M1::f1() }

//# publish --upgradeable --dependencies DepV1 --sender A
// Upgradeable consumer: published vs Dep v1, upgraded to v2 (fresh publish can't depend on a
// non-root version). Public fn => deps pinned at_least => calling v2 pins Dep at_least 2.
module DepConsumerV1::M;
public fun consume() { DepV1::M1::f1() }

//# upgrade --package DepConsumerV1 --upgrade-capability 6,1 --dependencies DepV2 --sender A
module DepConsumerV2::M;
public fun consume() { DepV2::M1::f2() }


//# programmable --sender A --inputs 10 @A
// publish pins DepV2 exact; direct call pins DepV1 exact => exact/exact conflict.
//> 0: Publish(Test, [DepV2, sui, std]);
//> 1: DepV1::M1::f1();
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 10 @A
// As above, order independent.
//> 0: DepV1::M1::f1();
//> 1: Publish(Test, [DepV2, sui, std]);
//> TransferObjects([Result(1)], Input(1))

//# programmable --sender A --inputs 10 @A
// Conflict via a dependency, not the root target: V1Consumer is not a transitive dep of Test, but
// its private entry pins DepV1 exact vs publish's DepV2 exact => exact/exact conflict.
//> 0: Publish(Test, [DepV2, sui, std]);
//> 1: V1Consumer::M::consume_v1();
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 10 @A
// As above, order independent.
//> 0: V1Consumer::M::consume_v1();
//> 1: Publish(Test, [DepV2, sui, std]);
//> TransferObjects([Result(1)], Input(1))

//# programmable --sender A --inputs 10 @A
// publish pins DepV1 exact (v1); public-fn dep pins Dep at_least 2 => exact(1) < at_least(2) =>
// Exact/AtLeast conflict.
//> 0: Publish(Test, [DepV1, sui, std]);
//> 1: DepConsumerV2::M::consume();
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 10 @A
// As above, order independent.
//> 0: DepConsumerV2::M::consume();
//> 1: Publish(Test, [DepV1, sui, std]);
//> TransferObjects([Result(1)], Input(1))

//# programmable --sender A --inputs 10 @A
// No `init` => publish contributes no constraint; DepV2 dep list is inert. Direct DepV1 call is the
// sole constraint => succeeds.
//> 0: Publish(NoInit, [DepV2, sui, std]);
//> 1: DepV1::M1::f1();
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs @A
// Two init publishes in one PTB: DepV1 exact + DepV2 exact => exact/exact conflict.
//> 0: Publish(Test, [DepV1, sui, std]);
//> 1: Publish(Test, [DepV2, sui, std]);
//> TransferObjects([Result(0), Result(1)], Input(0))

//# programmable --sender A --inputs @A
// As above but the DepV2 publish has no `init` => inert => only DepV1 exact => succeeds.
//> 0: Publish(Test, [DepV1, sui, std]);
//> 1: Publish(NoInit, [DepV2, sui, std]);
//> TransferObjects([Result(0), Result(1)], Input(0))
