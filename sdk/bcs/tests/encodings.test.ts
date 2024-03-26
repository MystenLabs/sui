// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';

import { bcs, fromB58, fromB64, fromHEX, toHEX } from './../src/index';

describe('BCS: Encodings', () => {
	it('should de/ser hex, base58 and base64', () => {
		expect(bcs.u8().parse(fromB64('AA=='))).toEqual(0);
		expect(bcs.u8().parse(fromHEX('00'))).toEqual(0);
		expect(bcs.u8().parse(fromB58('1'))).toEqual(0);

		const STR = 'this is a test string';
		const str = bcs.string().serialize(STR);

		expect(bcs.string().parse(fromB58(str.toBase58()))).toEqual(STR);
		expect(bcs.string().parse(fromB64(str.toBase64()))).toEqual(STR);
		expect(bcs.string().parse(fromHEX(str.toHex()))).toEqual(STR);
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
