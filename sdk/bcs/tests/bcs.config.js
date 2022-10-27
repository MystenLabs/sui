// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const { BCS, getRustConfig, getSuiMoveConfig } = require('../dist');

// default configurations
{
    {
        const bcs = new BCS(getRustConfig());
        let value = [ 'beep', 'boop', 'beep' ];
        assert_deep_equal(
            serde(bcs, 'Vec<string>', value),
            value,
            'rust config test'
        );
    };

    {
        const bcs = new BCS(getSuiMoveConfig());
        let value = [ 'beep', 'boop', 'beep' ];
        assert_deep_equal(
            serde(bcs, 'vector<string>', value),
            value,
            'rust config test'
        );
    };
};

// testing custom configuration (separators, vectorType)
{
    const bcs = new BCS({
        genericSeparators: ['[', ']'],
        addressLength: 1,
        addressEncoding: 'hex',
        vectorType: 'array',
        types: {
            structs: {
                SiteConfig: { tags: 'array[string]' }
            },
            enums: {
                'Option[T]': { none: null, some: 'T' }
            }
        }
    });

    {
        let value = { tags: [ 'beep', 'boop', 'beep' ] };
        assert_deep_equal(
            serde(bcs, 'SiteConfig', value),
            value,
            'struct definition config'
        );
    };

    {
        let value = { some: [ 'what', 'do', 'we', 'test' ]};
        assert_deep_equal(
            serde(bcs, 'Option[array[string]]', value),
            value,
            'enum definition config'
        );
    };
};

// forking
{
   let bcs_v1 = new BCS(getSuiMoveConfig());
   bcs_v1.registerStructType('User', { name: 'string' });

   let bcs_v2 = bcs_v1.fork();
   bcs_v2.registerStructType('Worker', { user: 'User', experience: 'u64' });

   assert(!bcs_v1.hasType('Worker'), 'fork');
   assert(bcs_v2.hasType('Worker'), 'fork');
}

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
