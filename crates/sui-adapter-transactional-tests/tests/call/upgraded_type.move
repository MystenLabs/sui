// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses TestRoot=0x0 TestDepV1=0x0 TestDepV2=0x0 TestDepV3=0x0 --accounts A

//# publish --upgradeable --sender A
module TestDepV1::m;
public struct V1()

//# upgrade --package TestDepV1 --upgrade-capability 1,1 --sender A
module TestDepV2::m;
public struct V1()
public struct V2()

//# upgrade --package TestDepV2 --upgrade-capability 1,1 --sender A
module TestDepV3::m;
public struct V1()
public struct V2()
public struct V3()

//# publish --upgradeable --dependencies TestDepV2 --sender A
module TestRoot::m;

public fun pub_fun<T>() {}

entry fun entry_fun<T>() {}

//# run TestRoot::m::pub_fun --type-args TestDepV1::m::V1

//# run TestRoot::m::entry_fun --type-args TestDepV1::m::V1

//# run TestRoot::m::pub_fun --type-args TestDepV2::m::V2

//# run TestRoot::m::entry_fun --type-args TestDepV2::m::V2

//# run TestRoot::m::pub_fun --type-args TestDepV3::m::V3

//# run TestRoot::m::entry_fun --type-args TestDepV3::m::V3
