// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';

import {
	BCS,
	fromB58,
	fromB64,
	fromHEX,
	getSuiMoveConfig,
	toB58,
	toB64,
	toHEX,
} from './../src/index';

describe('BCS: Encodings', () => {
	it('should de/ser hex, base58 and base64', () => {
		const bcs = new BCS(getSuiMoveConfig());

		expect(bcs.de('u8', 'AA==', 'base64')).toEqual(0);
		expect(bcs.de('u8', '00', 'hex')).toEqual(0);
		expect(bcs.de('u8', '1', 'base58')).toEqual(0);

		const STR = 'this is a test string';
		const str = bcs.ser('string', STR);

		expect(bcs.de('string', fromB58(str.toString('base58')), 'base58')).toEqual(STR);
		expect(bcs.de('string', fromB64(str.toString('base64')), 'base64')).toEqual(STR);
		expect(bcs.de('string', fromHEX(str.toString('hex')), 'hex')).toEqual(STR);
	});

	it('should de/ser native encoding types', () => {
		const bcs = new BCS(getSuiMoveConfig());

		bcs.registerStructType('TestStruct', {
			hex: BCS.HEX,
			base58: BCS.BASE58,
			base64: BCS.BASE64,
		});

		let hex_str = toHEX(new Uint8Array([1, 2, 3, 4, 5, 6]));
		let b58_str = toB58(new Uint8Array([1, 2, 3, 4, 5, 6]));
		let b64_str = toB64(new Uint8Array([1, 2, 3, 4, 5, 6]));

		let serialized = bcs.ser('TestStruct', {
			hex: hex_str,
			base58: b58_str,
			base64: b64_str,
		});

		let deserialized = bcs.de('TestStruct', serialized.toBytes());

		expect(deserialized.hex).toEqual(hex_str);
		expect(deserialized.base58).toEqual(b58_str);
		expect(deserialized.base64).toEqual(b64_str);
	});

	it('should deserialize hex with leading 0s', () => {
		const addressLeading0 = 'a7429d7a356dd98f688f11a330a32e0a3cc1908734a8c5a5af98f34ec93df0c';
		expect(toHEX(Uint8Array.from([0, 1]))).toEqual('0001');
		expect(fromHEX('0x1')).toEqual(Uint8Array.from([1]));
		expect(fromHEX('1')).toEqual(Uint8Array.from([1]));
		expect(fromHEX('111')).toEqual(Uint8Array.from([1, 17]));
		expect(fromHEX('001')).toEqual(Uint8Array.from([0, 1]));
		expect(fromHEX('011')).toEqual(Uint8Array.from([0, 17]));
		expect(fromHEX('0011')).toEqual(Uint8Array.from([0, 17]));
		expect(fromHEX('0x0011')).toEqual(Uint8Array.from([0, 17]));
		expect(fromHEX(addressLeading0)).toEqual(
			Uint8Array.from([
				10, 116, 41, 215, 163, 86, 221, 152, 246, 136, 241, 26, 51, 10, 50, 224, 163, 204, 25, 8,
				115, 74, 140, 90, 90, 249, 143, 52, 236, 147, 223, 12,
			]),
		);
		expect(toHEX(fromHEX(addressLeading0))).toEqual(`0${addressLeading0}`);
	});
});
