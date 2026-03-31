// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests function type parameter ability constraint violations

//# init --addresses test=0x0

//# publish
module test::m;

public struct NoCopy has drop {}
public struct NoDrop has copy {}
public struct NoStore has copy, drop {}
public struct HasAll has copy, drop, store {}

entry fun needs_copy<T: copy>() {}
entry fun needs_drop<T: drop>() {}
entry fun needs_store<T: store>() {}
entry fun needs_copy_drop<T: copy + drop>() {}
entry fun needs_copy_drop_store<T: copy + drop + store>() {}

// NoCopy has drop but not copy
//# run test::m::needs_copy --type-args test::m::NoCopy

// NoDrop has copy but not drop
//# run test::m::needs_drop --type-args test::m::NoDrop

// NoStore has copy+drop but not store
//# run test::m::needs_store --type-args test::m::NoStore

// needs_copy_drop: NoCopy missing copy
//# run test::m::needs_copy_drop --type-args test::m::NoCopy

// needs_copy_drop: NoDrop missing drop
//# run test::m::needs_copy_drop --type-args test::m::NoDrop

// needs_copy_drop_store: NoStore missing store
//# run test::m::needs_copy_drop_store --type-args test::m::NoStore

// happy paths
//# run test::m::needs_copy --type-args u64

//# run test::m::needs_copy_drop --type-args u64

//# run test::m::needs_copy_drop_store --type-args test::m::HasAll
