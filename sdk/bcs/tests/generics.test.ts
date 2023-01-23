// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';
import { BCS, fromB64, getSuiMoveConfig } from './../src/index';

describe('Generics BCS', () => {
    it('should handle generics', () => {
        const bcs = new BCS(getSuiMoveConfig());

        bcs.registerEnumType('Option<T>', {
            none: null,
            some: 'T',
        });

        expect(bcs.de('Option<u8>', '00', 'hex')).toEqual({ none: true });
    });

    it('should handle nested generics', () => {
        const bcs = new BCS(getSuiMoveConfig());

        bcs.registerEnumType('Option<T>', {
            none: null,
            some: 'T',
        });

        bcs.registerStructType('Container<T, S>', {
            tag: 'T',
            data: 'Option<S>'
        });

        expect(bcs.de('Container<bool, u8>', '0000', 'hex')).toEqual({
            tag: false,
            data: { none: true }
        });
    });

});
