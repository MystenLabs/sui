// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests value argument count mismatches

//# init --addresses test=0x0

//# publish
module test::m;

entry fun no_args() {}
entry fun one_arg(_x: u64) {}
entry fun two_args(_x: u64, _y: bool) {}

// 0 params, 1 arg
//# run test::m::no_args --args 42u64

// 1 param, 0 args
//# run test::m::one_arg

// 2 params, 3 args
//# run test::m::two_args --args 42u64 true false

// happy paths
//# run test::m::no_args

//# run test::m::one_arg --args 42u64

//# run test::m::two_args --args 42u64 true
