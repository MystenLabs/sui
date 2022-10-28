// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it } from 'vitest';
import { BCS, getSuiMoveConfig} from './../src/index';

describe('serde', () => {
    it('should serialize primitives in both directions', () => {
        const bcs = new BCS(getSuiMoveConfig());

        assert(serde(bcs, 'u8', '0').toString(10) === '0', 'u8');
        assert(serde(bcs, 'u8', '200').toString(10) === '200', 'u8');
        assert(serde(bcs, 'u8', '255').toString(10) === '255', 'u8');

        assert(serde(bcs, 'u16', '10000').toString(10) === '10000', 'u16');
        assert(serde(bcs, 'u32', '10000').toString(10) === '10000', 'u32');
        assert(serde(bcs, 'u256', '10000').toString(10) === '10000', 'u256');

        assert(bcs.de('u256', 'a086010000000000000000000000000000000000000000000000000000000000', 'hex').toString(10));
        assert(bcs.ser('u256', '100000').toString('hex') === 'a086010000000000000000000000000000000000000000000000000000000000', 'u256');


        assert(serde(bcs, 'u64', '1000').toString(10) === '1000', 'u64');
        assert(serde(bcs, 'u128', '1000').toString(10) === '1000', 'u128');
        assert(serde(bcs, 'u256', '1000').toString(10) === '1000', 'u256');

        assert(serde(bcs, 'bool', true) === true, 'bool');
        assert(serde(bcs, 'bool', false) === false, 'bool');

        assert(
            serde(bcs, 'address', '0xe3edac2c684ddbba5ad1a2b90fb361100b2094af')
            === 'e3edac2c684ddbba5ad1a2b90fb361100b2094af',
        'address');
    });

    it('should serde structs', () => {
        let bcs = new BCS(getSuiMoveConfig());

        bcs.registerAddressType('address', 20, 'hex');
        bcs.registerStructType('Beep', { id: 'address', value: 'u64' });

        let bytes = bcs.ser('Beep', { id: '0x45aacd9ed90a5a8e211502ac3fa898a3819f23b2', value: 10000000 }).toBytes();
        let struct = bcs.de('Beep', bytes);

        assert(struct.id === '45aacd9ed90a5a8e211502ac3fa898a3819f23b2');
        assert(struct.value.toString(10) === '10000000');
    });

    it('should serde enums', () => {
        let bcs = new BCS(getSuiMoveConfig());
        bcs.registerAddressType('address', 20, 'hex');
        bcs.registerEnumType('Enum', {
            with_value: 'address',
            no_value: null
        });

        let addr = '45aacd9ed90a5a8e211502ac3fa898a3819f23b2';

        assert(addr === bcs.de('Enum', bcs.ser('Enum', { with_value: addr }).toBytes()).with_value);
        assert('no_value' in bcs.de('Enum', bcs.ser('Enum', { no_value: null }).toBytes()));
    });

    it('should serde vectors natively', () => {
        let bcs = new BCS(getSuiMoveConfig());

        {
            let value = ['0', '255', '100'];
            assert_deep_equal(
                serde(bcs, 'vector<u8>', value).map((e) => e.toString(10)),
                value,
                'vector<u8>'
            );
        };

        {
            let value = ['100000', '555555555', '1123123', '0', '1214124124214'];
            assert_deep_equal(
                serde(bcs, 'vector<u64>', value).map((e) => e.toString(10)),
                value,
                'vector<u64>'
            );
        };

        {
            let value = ['100000', '555555555', '1123123', '0', '1214124124214'];
            assert_deep_equal(
                serde(bcs, 'vector<u128>', value).map((e) => e.toString(10)),
                value,
                'vector<u128>'
            );
        };

        {
            let value = [true, false, false, true, false];
            assert_deep_equal(
                serde(bcs, 'vector<bool>', value),
                value,
                'vector<bool>'
            );
        };

        {
            let value = [
                'e3edac2c684ddbba5ad1a2b90fb361100b2094af',
                '0000000000000000000000000000000000000001',
                '0000000000000000000000000000000000000002',
                'c0ffeec0ffeec0ffeec0ffeec0ffeec0ffee1337',
            ];

            assert_deep_equal(
                serde(bcs, 'vector<address>', value),
                value,
                'vector<address>'
            );
        };

        {
            let value = [
                [ true, false, true, true ],
                [ true, true, false, true ],
                [ false, true, true, true ],
                [ true, true, true, false ]
            ];

            assert_deep_equal(
                serde(bcs, 'vector<vector<bool>>', value),
                value,
                'vector<vector<bool>>'
            );
        };
    });

    it('should structs and nested enums', () => {
        let bcs = new BCS(getSuiMoveConfig());

        bcs.registerStructType('User', { age: 'u64', name: 'string' });
        bcs.registerStructType('Coin<T>', { balance: 'Balance<T>' });
        bcs.registerStructType('Balance<T>', { value: 'u64' });

        bcs.registerStructType('Container<T>', {
            owner: 'address',
            is_active: 'bool',
            item: 'T'
        });

        {
            let value = { age: '30', name: 'Bob' };
            assert(serde(bcs, 'User', value).age.toString(10) === value.age, 'User Struct');
            assert(serde(bcs, 'User', value).name === value.name, 'User Struct');
        };

        {
            let value = {
                owner: '0000000000000000000000000000000000000001',
                is_active: true,
                item: { balance: { value: '10000' } }
            };

            // Deep Nested Generic!
            let result = serde(bcs, 'Container<Coin<Balance<T>>>', value);

            assert(result.owner == value.owner, 'generic struct');
            assert(result.is_active == value.is_active, 'generic struct');
            assert(result.item.balance.value == value.item.balance.value, 'generic struct');
        };
    });
});

function serde(bcs, type, data) {
    let ser = bcs.ser(type, data).toString('hex');
    let de = bcs.de(type, ser, 'hex');
    return de;
}

function assert(cond, tag = '') {
    if (!cond) {
        throw new Error(`Assertion failed! Tag: ${tag}`);
    }
}

function assert_deep_equal(v1, v2, tag = '') {
    return assert(JSON.stringify(v1) == JSON.stringify(v2), tag);
}
