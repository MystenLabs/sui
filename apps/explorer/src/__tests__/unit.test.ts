// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { timeAgo } from '@mysten/core';
import { describe, it, expect } from 'vitest';

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
});
