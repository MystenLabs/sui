// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test=0x0

//# publish
module Test::M1 {
   public struct Event has copy, drop, store {
       x: u64,
   }

   fun init(_ctx: &mut TxContext) { 
       sui::event::emit(Event { x: 1 });
   }
}

//# view-object 1,0
