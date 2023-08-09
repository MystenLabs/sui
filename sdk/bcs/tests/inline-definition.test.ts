// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';
import { BCS, getSuiMoveConfig } from '../src/index';
import { serde } from './utils';

describe('BCS: Inline struct definitions', () => {
	it('should de/serialize inline definition', () => {
		const bcs = new BCS(getSuiMoveConfig());
		const value = {
			t1: 'Adam',
			t2: '1000',
			t3: ['aabbcc', '00aa00', '00aaffcc'],
		};

		expect(
			serde(
				bcs,
				{
					t1: 'string',
					t2: 'u64',
					t3: 'vector<hex-string>',
				},
				value,
			),
		).toEqual(value);
	});

	it('should not contain a trace of the temp struct', () => {
		const bcs = new BCS(getSuiMoveConfig());
		const _sr = bcs
			.ser({ name: 'string', age: 'u8' }, { name: 'Charlie', age: 10 })
			.toString('hex');

		expect(bcs.hasType('temp-struct')).toBe(false);
	});

	it('should avoid duplicate key', () => {
		const bcs = new BCS(getSuiMoveConfig());

		bcs.registerStructType('temp-struct', { a0: 'u8' });

		const sr = serde(bcs, { b0: 'temp-struct' }, { b0: { a0: 0 } });

		expect(bcs.hasType('temp-struct')).toBe(true);
		expect(sr).toEqual({ b0: { a0: 0 } });
	});
});
