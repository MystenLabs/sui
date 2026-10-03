// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun many_ctx(_: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext) {}

//# programmable
// VALID: twenty `&TxContext` parameters on their own
//> 0: test::m::many_ctx();
