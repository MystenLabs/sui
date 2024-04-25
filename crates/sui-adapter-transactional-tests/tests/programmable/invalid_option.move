// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests various invalid vector instantions for types

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public entry fun option_prim<T: copy + drop>(opt: Option<T>) {
        if (option::is_some(&opt)) {
            option::destroy_some(opt);
        }
    }
}

//# programmable --inputs vector[false,true]
//> test::m1::option_prim<bool>(Input(0));

//# programmable --inputs vector[0u8,0u8]
//> test::m1::option_prim<u8>(Input(0));

//# programmable --inputs vector[0u16,0u16]
//> test::m1::option_prim<u16>(Input(0));

//# programmable --inputs vector[0u32,0u32]
//> test::m1::option_prim<u32>(Input(0));

//# programmable --inputs vector[0u64,0u64]
//> test::m1::option_prim<u64>(Input(0));

//# programmable --inputs vector[0u128,0u128]
//> test::m1::option_prim<u128>(Input(0));

//# programmable --inputs vector[0u256,0u256]
//> test::m1::option_prim<u256>(Input(0));

//# programmable --inputs vector[@0,@0]
//> test::m1::option_prim<address>(Input(0));

//# programmable --inputs vector[@0,@0]
//> test::m1::option_prim<sui::object::ID>(Input(0));


// vectors

//# programmable --inputs vector[vector[],vector[]]
//> test::m1::option_prim<vector<bool>>(Input(0));

//# programmable --inputs vector[vector[],vector[]]
//> test::m1::option_prim<vector<u8>>(Input(0));

//# programmable --inputs vector[vector[],vector[]]
//> test::m1::option_prim<vector<u16>>(Input(0));

//# programmable --inputs vector[vector[],vector[]]
//> test::m1::option_prim<vector<u32>>(Input(0));

//# programmable --inputs vector[vector[],vector[]]
//> test::m1::option_prim<vector<u64>>(Input(0));

//# programmable --inputs vector[vector[],vector[]]
//> test::m1::option_prim<vector<u128>>(Input(0));

//# programmable --inputs vector[vector[],vector[]]
//> test::m1::option_prim<vector<u256>>(Input(0));

//# programmable --inputs vector[vector[],vector[]]
//> test::m1::option_prim<vector<address>>(Input(0));

//# programmable --inputs vector[vector[],vector[]]
//> test::m1::option_prim<vector<sui::object::ID>>(Input(0));
