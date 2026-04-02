// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests where type instantiation and value BCS encoding are inconsistent

//# init --addresses test=0x0

//# publish
module test::m;

entry fun take_val<T: copy + drop>(_x: T) {}
entry fun take_vec<T: copy + drop>(_x: vector<T>) {}
entry fun take_two<T: copy + drop>(_x: T, _y: T) {}

// T=bool but value is u64
//# run test::m::take_val --type-args bool --args 42u64

// T=u64 but value is bool
//# run test::m::take_val --type-args u64 --args true

// T=bool but vector contains u64 elements
//# run test::m::take_vec --type-args bool --args vector[42u64]

// T=u64, first arg correct, second arg is bool
//# run test::m::take_two --type-args u64 --args 42u64 true

// happy paths
//# run test::m::take_val --type-args u64 --args 42u64

//# run test::m::take_vec --type-args u64 --args vector[42u64]

//# run test::m::take_two --type-args u64 --args 42u64 42u64
