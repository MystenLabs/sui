// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from './../src/index';

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
        bcs.registerAddressType('address', 20, 'hex');
        bcs.registerStructType('Beep', { id: 'address', value: 'u64' });

        let bytes = bcs.ser('Beep', { id: '0x45aacd9ed90a5a8e211502ac3fa898a3819f23b2', value: 10000000 }).toBytes();
        let struct = bcs.de('Beep', bytes);

        assert(struct.id === '45aacd9ed90a5a8e211502ac3fa898a3819f23b2');
        assert(struct.value.toString(10) === '10000000');
    });

    test('it should serde enums', () => {
        bcs.registerAddressType('address', 20, 'hex');
        bcs.registerEnumType('Enum', {
            with_value: 'address',
            no_value: null
        });

        let addr = '45aacd9ed90a5a8e211502ac3fa898a3819f23b2';

        assert(addr === bcs.de('Enum', bcs.ser('Enum', { with_value: addr }).toBytes()).with_value);
        assert('no_value' in bcs.de('Enum', bcs.ser('Enum', { no_value: null }).toBytes()));
    });
});

function assert(cond: boolean) {
    expect(cond).toBeTruthy();
}
