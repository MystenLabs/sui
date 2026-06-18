// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Type-argument packages are pinned `at_least` (version from the type's defining id), for both
// MoveCall type args and the MakeMoveVec element type.

//# init --addresses Test=0x0 DepV1=0x0 DepV2=0x0 Gen=0x0 --accounts A

//# publish --upgradeable --sender A
module DepV1::M1;
public struct A has drop { }

// `B` introduced in v2 => its defining id is v2 => a `B` type-arg pins Dep at_least 2.
//# upgrade --package DepV1 --upgrade-capability 1,1 --sender A
module DepV2::M1;
public struct A has drop { }
public struct B has drop { }

//# publish --sender A
module Gen::m;
public fun id<T>() { }

//# stage-package
module Test::M;
fun init(_ctx: &mut TxContext) { }

// publish pins DepV1 exact (v1); B type-arg pins Dep at_least 2 => exact(1) < at_least(2) =>
// Exact/AtLeast conflict.
//# programmable --sender A --inputs @A
//> 0: Publish(Test, [DepV1, sui, std]);
//> 1: Gen::m::id<DepV2::M1::B>();
//> TransferObjects([Result(0)], Input(0))

// Same B type as MakeMoveVec element: pins Dep at_least 2 => exact(1) < at_least(2) =>
// Exact/AtLeast conflict.
//# programmable --sender A --inputs @A
//> 0: Publish(Test, [DepV1, sui, std]);
//> 1: MakeMoveVec<DepV2::M1::B>([]);
//> TransferObjects([Result(0)], Input(0))
