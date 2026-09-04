// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

use sui::coin::Coin;
use sui::sui::SUI;

public fun id_mut<T>(t: &mut T): &mut T { t }
public fun value_at_least(c: &Coin<SUI>, v: u64) { assert!(c.value() >= v, 0) }
public fun use_mut<T>(_: &mut T) {}

//# programmable --sender A --inputs 1000 @B
// VALID: SplitCoins through a `&mut Coin<SUI>` result rooted in the gas coin
//> 0: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 1: SplitCoins(Result(0), [Input(0)]);
//> 2: TransferObjects([Result(1)], Input(1));

//# view-object 2,0

//# programmable --sender A --inputs 1000 @B
// VALID: `&mut Balance<SUI>` from the gas coin, split and transferred
//> 0: sui::coin::balance_mut<sui::sui::SUI>(Gas);
//> 1: sui::balance::split<sui::sui::SUI>(Result(0), Input(0));
//> 2: sui::coin::from_balance<sui::sui::SUI>(Result(1));
//> 3: TransferObjects([Result(2)], Input(1));

//# view-object 4,0

//# programmable --sender A --inputs 1000
// VALID: `&Coin<SUI>` from a `&mut` gas reference (freeze)
//> 0: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 1: test::m::value_at_least(Result(0), Input(0));

//# programmable --sender A --inputs @B
// INVALID: CannotMoveBorrowedValue at arg 0 of command 1, gas transferred while borrowed
//> 0: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 1: TransferObjects([Gas], Input(0));
//> 2: test::m::use_mut<sui::coin::Coin<sui::sui::SUI>>(Result(0));

//# programmable --sender A --inputs 1000 @B
// INVALID: CannotWriteToExtendedReference at arg 0 of command 1, split while a balance reference is live
//> 0: sui::coin::balance_mut<sui::sui::SUI>(Gas);
//> 1: SplitCoins(Gas, [Input(0)]);
//> 2: TransferObjects([Result(1)], Input(1));
//> 3: test::m::use_mut<sui::balance::Balance<sui::sui::SUI>>(Result(0));

//# programmable --sender A --inputs @B
// VALID: the gas coin is drained through a reference; the budget refund still lands
//> 0: sui::coin::balance_mut<sui::sui::SUI>(Gas);
//> 1: sui::balance::withdraw_all<sui::sui::SUI>(Result(0));
//> 2: sui::coin::from_balance<sui::sui::SUI>(Result(1));
//> 3: TransferObjects([Result(2)], Input(0));

//# view-object 0,0

//# view-object 9,0
