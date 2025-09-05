// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test=0x0

//# publish
module Test::M1 {
   public struct X has key {
       id: UID,
   }

   fun init(ctx: &mut TxContext) { 
       sui::transfer::transfer(X { id: object::new(ctx) }, ctx.sender());
   }
}

//# view-object 1,1
