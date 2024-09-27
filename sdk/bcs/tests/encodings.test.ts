// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';

import { bcs, fromBase58, fromBase64, fromHex, toHex } from './../src/index';

describe('BCS: Encodings', () => {
	it('should de/ser hex, base58 and base64', () => {
		expect(bcs.u8().parse(fromBase64('AA=='))).toEqual(0);
		expect(bcs.u8().parse(fromHex('00'))).toEqual(0);
		expect(bcs.u8().parse(fromBase58('1'))).toEqual(0);

		const STR = 'this is a test string';
		const str = bcs.string().serialize(STR);

		expect(bcs.string().parse(fromBase58(str.toBase58()))).toEqual(STR);
		expect(bcs.string().parse(fromBase64(str.toBase64()))).toEqual(STR);
		expect(bcs.string().parse(fromHex(str.toHex()))).toEqual(STR);
	});

	it('should deserialize hex with leading 0s', () => {
		const addressLeading0 = 'a7429d7a356dd98f688f11a330a32e0a3cc1908734a8c5a5af98f34ec93df0c';
		expect(toHex(Uint8Array.from([0, 1]))).toEqual('0001');
		expect(fromHex('0x1')).toEqual(Uint8Array.from([1]));
		expect(fromHex('1')).toEqual(Uint8Array.from([1]));
		expect(fromHex('111')).toEqual(Uint8Array.from([1, 17]));
		expect(fromHex('001')).toEqual(Uint8Array.from([0, 1]));
		expect(fromHex('011')).toEqual(Uint8Array.from([0, 17]));
		expect(fromHex('0011')).toEqual(Uint8Array.from([0, 17]));
		expect(fromHex('0x0011')).toEqual(Uint8Array.from([0, 17]));
		expect(fromHex(addressLeading0)).toEqual(
			Uint8Array.from([
				10, 116, 41, 215, 163, 86, 221, 152, 246, 136, 241, 26, 51, 10, 50, 224, 163, 204, 25, 8,
				115, 74, 140, 90, 90, 249, 143, 52, 236, 147, 223, 12,
			]),
		);
		expect(toHex(fromHex(addressLeading0))).toEqual(`0${addressLeading0}`);
	});
});
