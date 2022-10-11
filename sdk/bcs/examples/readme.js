// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const { bcs } = require('./../dist');

{
    // BCS has a set of built ins:
    // U8, U32, U64, U128, BOOL, STRING
    console.assert(bcs.U64 === 'u64');
    console.assert(bcs.BOOL === 'bool');
    console.assert(bcs.STRING === 'string');

    // De/serialization of primitives is included by default;
    let u8 = bcs.de(bcs.U8, '00', 'hex'); // '0'
    let u32 = bcs.de(bcs.U32, '78563412', 'hex'); // '78563412'
    let u64 = bcs.de(bcs.U64, 'ffffffffffffffff', 'hex'); // '18446744073709551615'
    let u128 = bcs.de(bcs.U128, 'FFFFFFFF000000000000000000000000', 'hex'); // '4294967295'
    let bool = bcs.de(bcs.BOOL, '00', 'hex'); // false

    // There's also a handy built-in for ASCII strings (which are `vector<u8>` under the hood)
    let str = bcs.de(bcs.STRING, '0a68656c6c6f5f6d6f7665', 'hex'); // hello_move

    console.log(str);
}


{
    let bcs_u8 = bcs.ser('u8', 255).toString('hex'); // uint Array
    console.assert(bcs_u8 === 'ff');

    let bcs_ascii = bcs.ser('string', 'hello_move').toString('hex');
    console.assert(bcs_ascii === '0a68656c6c6f5f6d6f7665');
}

{
    // Move / Rust struct
    // struct Coin {
    //   value: u64,
    //   owner: vector<u8>, // name // Vec<u8> in Rust
    //   is_locked: bool,
    // }

    bcs.registerStructType('Coin', {
        value: bcs.U64,
        owner: bcs.STRING,
        is_locked: bcs.BOOL
    });

    // Created in Rust with diem/bcs
    let rust_bcs_str = '80d1b105600000000e4269672057616c6c65742047757900';

    console.log(bcs.de('Coin', rust_bcs_str, 'hex'));

    // Let's encode the value as well
    let test_ser = bcs.ser('Coin', {
        owner: 'Big Wallet Guy',
        value: '412412400000',
        is_locked: false
    });

    console.log(test_ser.toBytes());
    console.assert(test_ser.toString('hex') === rust_bcs_str, 'Whoopsie, result mismatch');
}
