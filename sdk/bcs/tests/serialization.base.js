// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const { BCS, getSuiMoveConfig } = require('../dist');
const bcs = new BCS(getSuiMoveConfig());

// testing all primitive Move values: u8, u64, u128, bool and address
// Serialization approach is ser and then de and compare.
{
    assert(serde(bcs, 'u8', '0').toString(10) === '0', 'u8');
    assert(serde(bcs, 'u8', '200').toString(10) === '200', 'u8');
    assert(serde(bcs, 'u8', '255').toString(10) === '255', 'u8');

    assert(serde(bcs, 'u64', '1000').toString(10) === '1000', 'u64');
    assert(serde(bcs, 'u128', '1000').toString(10) === '1000', 'u128');

    assert(serde(bcs, 'bool', true) === true, 'bool');
    assert(serde(bcs, 'bool', false) === false, 'bool');

    assert(
        serde(bcs, 'address', '0xe3edac2c684ddbba5ad1a2b90fb361100b2094af')
        === 'e3edac2c684ddbba5ad1a2b90fb361100b2094af',
    'address');
};

// Test vectors of Move's built-in types.
// Also does a vector<vector<bool>> test.
{
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
};

// Test structs
{
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
};


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
