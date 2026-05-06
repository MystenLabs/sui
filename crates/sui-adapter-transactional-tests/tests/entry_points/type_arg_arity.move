// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests function type argument count mismatches

//# init --addresses test=0x0

//# publish
module test::m;

entry fun no_ty_args() {}
entry fun one_ty_arg<T>() {}
entry fun two_ty_args<T, U>() {}

// non-generic function called with type args
//# run test::m::no_ty_args --type-args u64

// 1 type param, 0 type args
//# run test::m::one_ty_arg

// 1 type param, 2 type args
//# run test::m::one_ty_arg --type-args u64 bool

// 2 type params, 1 type arg
//# run test::m::two_ty_args --type-args u64

// 2 type params, 3 type args
//# run test::m::two_ty_args --type-args u64 bool u8

// happy paths
//# run test::m::no_ty_args

//# run test::m::one_ty_arg --type-args u64

//# run test::m::two_ty_args --type-args u64 bool
