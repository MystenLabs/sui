// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';
import { Ed25519PublicKey } from '../../../src';

// Test case generated against CLI:
// cargo build --bin sui
// ../sui/target/debug/sui client new-address ed25519
// ../sui/target/debug/sui keytool list
let TEST_CASES = new Map<string, string>([
	[
		'UdGRWooy48vGTs0HBokIis5NK+DUjiWc9ENUlcfCCBE=',
		'0xd77a6cd55073e98d4029b1b0b8bd8d88f45f343dad2732fc9a7965094e635c55',
	],
	[
		'0PTAfQmNiabgbak9U/stWZzKc5nsRqokda2qnV2DTfg=',
		'0x7e8fd489c3d3cd9cc7cbcc577dc5d6de831e654edd9997d95c412d013e6eea23',
	],
	[
		'6L/l0uhGt//9cf6nLQ0+24Uv2qanX/R6tn7lWUJX1Xk=',
		'0x3a1b4410ebe9c3386a429c349ba7929aafab739c277f97f32622b971972a14a2',
	],
]);

const VALID_KEY_BASE64 = 'Uz39UFseB/B38iBwjesIU1JZxY6y+TRL9P84JFw41W4=';

describe('Ed25519PublicKey', () => {
	it('invalid', () => {
		// public key length 33 is invalid for Ed25519
		expect(() => {
			new Ed25519PublicKey([
				3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0,
			]);
		}).toThrow();

		expect(() => {
			new Ed25519PublicKey(
				'0x300000000000000000000000000000000000000000000000000000000000000000000',
			);
		}).toThrow();

		expect(() => {
			new Ed25519PublicKey('0x300000000000000000000000000000000000000000000000000000000000000');
		}).toThrow();

		expect(() => {
			new Ed25519PublicKey(
				'135693854574979916511997248057056142015550763280047535983739356259273198796800000',
			);
		}).toThrow();

		expect(() => {
			new Ed25519PublicKey('12345');
		}).toThrow();
	});

	it('toBase64', () => {
		const key = new Ed25519PublicKey(VALID_KEY_BASE64);
		expect(key.toBase64()).toEqual(VALID_KEY_BASE64);
		expect(key.toString()).toEqual(VALID_KEY_BASE64);
	});

	it('toBuffer', () => {
		const key = new Ed25519PublicKey(VALID_KEY_BASE64);
		expect(key.toBytes().length).toBe(32);
		expect(new Ed25519PublicKey(key.toBytes()).equals(key)).toBe(true);
	});

	TEST_CASES.forEach((address, base64) => {
		it(`toSuiAddress from base64 public key ${address}`, () => {
			const key = new Ed25519PublicKey(base64);
			expect(key.toSuiAddress()).toEqual(address);
		});
	});
});
