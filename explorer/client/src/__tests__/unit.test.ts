// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import BN from 'bn.js';

import { presentBN } from '../utils/stringUtils';
import { timeAgo } from '../utils/timeUtils';

const timeNow = 1735693990000;

describe('Unit Tests', () => {
    describe('timeAgo', () => {
        it('handles days', () => {
            expect(timeAgo(1734220800000, timeNow)).toEqual('17 days 1 hour');
        });
        it('handles hours', () => {
            expect(timeAgo(1735610580000, timeNow)).toEqual('23 hours 10 mins');
        });
        it('handles minutes', () => {
            expect(timeAgo(1735693930000, timeNow)).toEqual('1 min');
        });
        it('handles seconds', () => {
            expect(timeAgo(1735693987000, timeNow)).toEqual('3 secs');
        });
        it('handles milliseconds', () => {
            expect(timeAgo(1735693989100, timeNow)).toEqual('< 1 sec');
        });
    });

    describe('presentBN', () => {
        it.each([
            [1, '1'],
            [10, '10'],
            [100, '100'],
            [1000, '1,000'],
            [10000, '10,000'],
            [100000, '100,000'],
            [1000000, '1,000,000'],
            [10000000, '10,000,000'],
        ])('handles increasing numbers', (input, output) => {
            expect(presentBN(new BN(input, 10))).toEqual(output);
        });
    });
});
