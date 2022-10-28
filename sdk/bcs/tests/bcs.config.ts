// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';
import { BCS, getRustConfig, getSuiMoveConfig } from "../src/index";

describe('serde', () => {
    it('should work with Rust config', () => {
        const bcs = new BCS(getRustConfig());
        let value = [ 'beep', 'boop', 'beep' ];
        assert_deep_equal(
            serde(bcs, 'Vec<string>', value),
            value,
            'rust config test'
        );
    });

    it('should work with Sui Move config', () => {
        const bcs = new BCS(getSuiMoveConfig());
        let value = [ 'beep', 'boop', 'beep' ];
        assert_deep_equal(
            serde(bcs, 'vector<string>', value),
            value,
            'rust config test'
        );
    });

    it('should fork config', () => {
        let bcs_v1 = new BCS(getSuiMoveConfig());
        bcs_v1.registerStructType('User', { name: 'string' });

        let bcs_v2 = new BCS(bcs_v1);
        bcs_v2.registerStructType('Worker', { user: 'User', experience: 'u64' });

        assert(!bcs_v1.hasType('Worker'), 'fork');
        assert(bcs_v2.hasType('Worker'), 'fork');
    });

    it('should work with custom config', () => {
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

        let value_1 = { tags: [ 'beep', 'boop', 'beep' ] };
        assert_deep_equal(
            serde(bcs, 'SiteConfig', value_1),
            value_1,
            'struct definition config'
        );

        let value_2 = { some: [ 'what', 'do', 'we', 'test' ]};
        assert_deep_equal(
            serde(bcs, 'Option[array[string]]', value_2),
            value_2,
            'enum definition config'
        );
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
