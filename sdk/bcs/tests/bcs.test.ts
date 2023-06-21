// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';
import { BCS, fromB64, getSuiMoveConfig } from './../src/index';

describe('BCS: Primitives', () => {
	it('should de/ser primitives: u8', () => {
		const bcs = new BCS(getSuiMoveConfig());

		expect(bcs.de('u8', fromB64('AQ=='))).toEqual(1);
		expect(bcs.de('u8', fromB64('AA=='))).toEqual(0);
	});

	it('should ser/de u64', () => {
		const bcs = new BCS(getSuiMoveConfig());

		const exp = 'AO/Nq3hWNBI=';
		const num = '1311768467750121216';
		const set = bcs.ser('u64', num).toString('base64');

		expect(set).toEqual(exp);
		expect(bcs.de('u64', exp, 'base64')).toEqual('1311768467750121216');
	});

	it('should ser/de u128', () => {
		const bcs = new BCS(getSuiMoveConfig());

		const sample = 'AO9ld3CFjD48AAAAAAAAAA==';
		const num = BigInt('1111311768467750121216');

		expect(bcs.de('u128', sample, 'base64').toString(10)).toEqual('1111311768467750121216');
		expect(bcs.ser('u128', num).toString('base64')).toEqual(sample);
	});

	it('should de/ser custom objects', () => {
		const bcs = new BCS(getSuiMoveConfig());

		bcs.registerStructType('Coin', {
			value: BCS.U64,
			owner: BCS.STRING,
			is_locked: BCS.BOOL,
		});

		const rustBcs = 'gNGxBWAAAAAOQmlnIFdhbGxldCBHdXkA';
		const expected = {
			owner: 'Big Wallet Guy',
			value: '412412400000',
			is_locked: false,
		};

		const setBytes = bcs.ser('Coin', expected);

		expect(bcs.de('Coin', fromB64(rustBcs))).toEqual(expected);
		expect(setBytes.toString('base64')).toEqual(rustBcs);
	});

	it('should de/ser vectors', () => {
		const bcs = new BCS(getSuiMoveConfig());

		// Rust-bcs generated vector with 1000 u8 elements (FF)
		const sample = largebcsVec();

		// deserialize data with JS
		const deserialized = bcs.de('vector<u8>', fromB64(sample));

		// create the same vec with 1000 elements
		let arr = Array.from(Array(1000)).map(() => 255);
		const serialized = bcs.ser('vector<u8>', arr);

		expect(deserialized.length).toEqual(1000);
		expect(serialized.toString('base64')).toEqual(largebcsVec());
	});

	it('should de/ser enums', () => {
		const bcs = new BCS(getSuiMoveConfig());

		bcs.registerStructType('Coin', { value: 'u64' });
		bcs.registerEnumType('Enum', {
			single: 'Coin',
			multi: 'vector<Coin>',
		});

		// prepare 2 examples from Rust bcs
		let example1 = fromB64('AICWmAAAAAAA');
		let example2 = fromB64('AQIBAAAAAAAAAAIAAAAAAAAA');

		// serialize 2 objects with the same data and signature
		let set1 = bcs.ser('Enum', { single: { value: 10000000 } }).toBytes();
		let set2 = bcs
			.ser('Enum', {
				multi: [{ value: 1 }, { value: 2 }],
			})
			.toBytes();

		// deserialize and compare results
		expect(bcs.de('Enum', example1)).toEqual(bcs.de('Enum', set1));
		expect(bcs.de('Enum', example2)).toEqual(bcs.de('Enum', set2));
	});

	it('should de/ser addresses', () => {
		const bcs = new BCS(
			Object.assign(getSuiMoveConfig(), {
				addressLength: 16,
				addressEncoding: 'hex',
			}),
		);

		// Move Kitty example:
		// Wallet { kitties: vector<Kitty>, owner: address }
		// Kitty { id: 'u8' }

		// bcs.registerAddressType('address', 16, 'base64'); // Move has 16/20/32 byte addresses
		bcs.registerStructType('Kitty', { id: 'u8' });
		bcs.registerStructType('Wallet', {
			kitties: 'vector<Kitty>',
			owner: 'address',
		});

		// Generated with Move CLI i.e. on the Move side
		let sample = 'AgECAAAAAAAAAAAAAAAAAMD/7g==';
		let data = bcs.de('Wallet', fromB64(sample));

		expect(data.kitties).toHaveLength(2);
		expect(data.owner).toEqual('00000000000000000000000000c0ffee');
	});

	it('should support growing size', () => {
		const bcs = new BCS(getSuiMoveConfig());

		bcs.registerStructType('Coin', {
			value: BCS.U64,
			owner: BCS.STRING,
			is_locked: BCS.BOOL,
		});

		const rustBcs = 'gNGxBWAAAAAOQmlnIFdhbGxldCBHdXkA';
		const expected = {
			owner: 'Big Wallet Guy',
			value: '412412400000',
			is_locked: false,
		};

		const setBytes = bcs.ser('Coin', expected, { size: 1, maxSize: 1024 });

		expect(bcs.de('Coin', fromB64(rustBcs))).toEqual(expected);
		expect(setBytes.toString('base64')).toEqual(rustBcs);
	});

	it('should error when attempting to grow beyond the allowed size', () => {
		const bcs = new BCS(getSuiMoveConfig());

		bcs.registerStructType('Coin', {
			value: BCS.U64,
			owner: BCS.STRING,
			is_locked: BCS.BOOL,
		});

		const expected = {
			owner: 'Big Wallet Guy',
			value: 412412400000n,
			is_locked: false,
		};

		expect(() => bcs.ser('Coin', expected, { size: 1 })).toThrowError();
	});
});

// @ts-ignore

function largebcsVec(): string {
	return '6Af/////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////';
}
