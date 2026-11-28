// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A mutable FreezeList input owes the 250_000_000 MIST pre-execution charge. A budget equal to
// that charge leaves no gas for input loading, so the transaction runs out of gas before its
// command can execute and consumes the whole budget. A larger budget executes successfully.

//# init --accounts A --addresses test=0x0

//# publish --sender A
module test::m;

use sui::coin_regulation::FreezeList;

public fun touch(_freeze_list: &mut FreezeList) {}

//# view-object 0,0

//# programmable --sender A --inputs mutshared(0xcf) --gas-budget 250000000
//> test::m::touch(Input(0));

//# view-object 0,0

// The additional budget covers normal input and execution gas after the pre-execution charge.
//# programmable --sender A --inputs mutshared(0xcf) --gas-budget 500000000
//> test::m::touch(Input(0));

//# view-object 0,0
