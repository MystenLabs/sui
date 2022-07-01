// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { BCS as bcs } from './../src/index';

describe('serde', () => {
    test('it should serialize primitives in both directions', () => {
        let u8 = 100;
        let u64 = '18446744073709551615';
        let u128 = '1844674407370955161518446744073709551';
        let str = 'beep-boop';
        let bool = true;

        assert(u8.toString(10) === bcs.de(bcs.U8, bcs.ser(bcs.U8, u8).toBytes()).toString(10));
        assert(u64 === bcs.de(bcs.U64, bcs.ser(bcs.U64, u64).toBytes()).toString(10));
        assert(u128 === bcs.de(bcs.U128, bcs.ser(bcs.U128, u128).toBytes()).toString(10));

        assert(str === bcs.de(bcs.STRING, bcs.ser(bcs.STRING, str).toBytes()));
        assert(bool === bcs.de(bcs.BOOL, bcs.ser(bcs.BOOL, bool).toBytes()))
    });

    test('it should serde structs', () => {
        bcs.registerAddressType('address', 20);
        bcs.registerStructType('Beep', { id: 'address', value: 'u64' });


        let struct = { id: '' }
    });
});

function assert(cond: boolean) {
    expect(cond).toBeTruthy();
}
