// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests cannot call `coin_registry::new_currency` with programmable transactions

//# init --addresses test=0x0 --accounts A

//# publish --sender A
module test::m1;

use std::string::String;

public fun name(): String {
  b"A".to_string()
}

//# programmable --sender A --inputs object(0xc) @A 6u8
//> 0: test::m1::name();
//> 1: sui::coin_registry::new_currency<sui::kiosk::Kiosk>(Input(0), Input(2), Result(0), Result(0), Result(0), Result(0));
//> 2: sui::coin_registry::finalize<sui::kiosk::Kiosk>(NestedResult(1,0));
//> 3: TransferObjects([NestedResult(1,1), Result(2)], Input(1));
