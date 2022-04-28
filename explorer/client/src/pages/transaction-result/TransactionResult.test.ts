// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { instanceOfDataType } from './TransactionResult';

describe('tests for Type Guard', () => {
    test('correct object passes', () => {
        const entry = {
            id: 'A1dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd',
            sender: '78b786a771e314eabc378d81c87c8777715b5e9e509b3b2bded677f14ad5931d',
            status: 'success',
        };
        expect(instanceOfDataType(entry)).toBe(true);
    });
});
