// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests invalid type args

//# init --addresses test=0x0 test1=0x0 --accounts A

//# publish --upgradeable --sender A
module test::m;

entry fun foo() {}

//# upgrade --package test --upgrade-capability 1,1 --sender A
module test1::m;

public fun bar() {}

// Cannot be resolved under unified linkage
//# programmable --sender A
//> 0: test::m::foo();
//> 1: test1::m::bar();

//# programmable --sender A
//> test::m::foo();

//# programmable --sender A
//> test1::m::bar();
