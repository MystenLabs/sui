// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun id_mut<T>(t: &mut T): &mut T { t }

public fun use_mut<T>(_: &mut T) {}

//# programmable --sender A --inputs 1000 100 @B
// VALID: the reference is still used after the split (an alias, not an extension)
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Result(0));
//> 2: SplitCoins(Result(1), [Input(1)]);
//> 3: test::m::use_mut<sui::coin::Coin<sui::sui::SUI>>(Result(1));
//> 4: TransferObjects([Result(0), Result(2)], Input(2));

//# programmable --sender A --inputs 1000 100 @B
// VALID: the extension is dead by the time of the split
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Result(0));
//> 2: sui::coin::balance_mut<sui::sui::SUI>(Result(1));
//> 3: test::m::use_mut<sui::balance::Balance<sui::sui::SUI>>(Result(2));
//> 4: SplitCoins(Result(1), [Input(1)]);
//> 5: TransferObjects([Result(0), Result(4)], Input(2));
