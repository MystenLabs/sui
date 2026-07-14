// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests documents the strange things that occur in dev inspect with an owned TxContext

//# init --addresses test=0x0

//# publish
module test::m;

public fun owned(ctx: TxContext): TxContext {
    ctx
}

public fun mut_id(ctx: &mut TxContext): &mut TxContext {
    ctx
}

public fun owned_mut(_: TxContext, _: &mut TxContext) {
}

//# programmable --dev-inspect --inputs struct(@0,vector[],0,0,0)
// Invalid, cannot specify inferred TxContext
//> test::m::mut_id(Input(0));


//# programmable --dev-inspect --inputs struct(@0,vector[],0,0,0)
// Invalid, cannot specify inferred TxContext
//> 0: test::m::owned(Input(0));
//> test::m::mut_id(Result(0));

//# programmable --dev-inspect --inputs struct(@0,vector[],0,0,0)
// Invalid, cannot specify inferred TxContext
//> test::m::owned_mut(Input(0), Input(0));

//# programmable --dev-inspect --inputs struct(@0,vector[],0,0,0)
// Invalid, cannot specify inferred TxContext
//> 0: test::m::owned(Input(0));
//> test::m::owned_mut(Result(0), Result(0));

//# programmable --dev-inspect --inputs struct(@0,vector[],0,0,0) struct(@0,vector[],0,0,0) struct(@0,vector[],0,0,0)
// Valid, owned TxContext usage is not inferred since it is impossible outside of dev inspect
//> 0: test::m::owned(Input(0));
//> 1: test::m::owned(Result(0));
//> 2: test::m::owned(Input(1));
//> test::m::owned_mut(Result(1));
//> test::m::owned_mut(Result(2));
//> test::m::owned_mut(Input(2));
