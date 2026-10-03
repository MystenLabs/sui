// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun two<A, B>(_: &mut A, _: &mut B) {}

//# programmable --inputs 0u8
// INVALID: InvalidReferenceArgument at arg 0 of command 0, one input at one type twice
//> 0: test::m::two<u8, u8>(Input(0), Input(0));
