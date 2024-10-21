// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';

import { bcs, fromBase64 } from './../src/index';

describe('BCS: Primitives', () => {
	it('should support growing size', () => {
		const Coin = bcs.struct('Coin', {
			value: bcs.u64(),
			owner: bcs.string(),
			is_locked: bcs.bool(),
		});

		const rustBcs = 'gNGxBWAAAAAOQmlnIFdhbGxldCBHdXkA';
		const expected = {
			owner: 'Big Wallet Guy',
			value: '412412400000',
			is_locked: false,
		};

		const setBytes = Coin.serialize(expected, { initialSize: 1, maxSize: 1024 });

		expect(Coin.parse(fromBase64(rustBcs))).toEqual(expected);
		expect(setBytes.toBase64()).toEqual(rustBcs);
	});

	it('should error when attempting to grow beyond the allowed size', () => {
		const Coin = bcs.struct('Coin', {
			value: bcs.u64(),
			owner: bcs.string(),
			is_locked: bcs.bool(),
		});

		const expected = {
			owner: 'Big Wallet Guy',
			value: 412412400000n,
			is_locked: false,
		};

		expect(() => Coin.serialize(expected, { initialSize: 1, maxSize: 1 })).toThrowError();
	});
});
