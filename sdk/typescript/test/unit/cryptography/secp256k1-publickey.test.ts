// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB64, toHEX } from '@mysten/bcs';
import { describe, it, expect } from 'vitest';
import { Secp256k1PublicKey } from '../../../src/cryptography/secp256k1-publickey';
import { INVALID_SECP256K1_PUBLIC_KEY, VALID_SECP256K1_PUBLIC_KEY } from './secp256k1-keypair.test';

// Test case generated against CLI:
// cargo build --bin sui
// ../sui/target/debug/sui client new-address secp256k1
// ../sui/target/debug/sui keytool list
let SECP_TEST_CASES = new Map<string, string>([
	[
		'AwTC3jVFRxXc3RJIFgoQcv486QdqwYa8vBp4bgSq0gsI',
		'0xcdce00b4326fb908fdac83c35bcfbda323bfcc0618b47c66ccafbdced850efaa',
	],
	[
		'A1F2CtldIGolO92Pm9yuxWXs5E07aX+6ZEHAnSuKOhii',
		'0xb588e58ed8967b6a6f9dbce76386283d374cf7389fb164189551257e32b023b2',
	],
	[
		'Ak5rsa5Od4T6YFN/V3VIhZ/azMMYPkUilKQwc+RiaId+',
		'0x694dd74af1e82b968822a82fb5e315f6d20e8697d5d03c0b15e0178c1a1fcfa0',
	],
	[
		'A4XbJ3fLvV/8ONsnLHAW1nORKsoCYsHaXv9FK1beMtvY',
		'0x78acc6ca0003457737d755ade25a6f3a144e5e44ed6f8e6af4982c5cc75e55e7',
	],
]);
describe('Secp256k1PublicKey', () => {
	it('invalid', () => {
		expect(() => {
			new Secp256k1PublicKey(INVALID_SECP256K1_PUBLIC_KEY);
		}).toThrow();

		expect(() => {
			const invalid_pubkey_buffer = new Uint8Array(INVALID_SECP256K1_PUBLIC_KEY);
			let invalid_pubkey_base64 = toB64(invalid_pubkey_buffer);
			new Secp256k1PublicKey(invalid_pubkey_base64);
		}).toThrow();

		expect(() => {
			const pubkey_buffer = new Uint8Array(VALID_SECP256K1_PUBLIC_KEY);
			let wrong_encode = toHEX(pubkey_buffer);
			new Secp256k1PublicKey(wrong_encode);
		}).toThrow();

		expect(() => {
			new Secp256k1PublicKey('12345');
		}).toThrow();
	});

	it('toBase64', () => {
		const pub_key = new Uint8Array(VALID_SECP256K1_PUBLIC_KEY);
		let pub_key_base64 = toB64(pub_key);
		const key = new Secp256k1PublicKey(pub_key_base64);
		expect(key.toBase64()).toEqual(pub_key_base64);
		expect(key.toString()).toEqual(pub_key_base64);
	});

	it('toBuffer', () => {
		const pub_key = new Uint8Array(VALID_SECP256K1_PUBLIC_KEY);
		let pub_key_base64 = toB64(pub_key);
		const key = new Secp256k1PublicKey(pub_key_base64);
		expect(key.toBytes().length).toBe(33);
		expect(new Secp256k1PublicKey(key.toBytes()).equals(key)).toBe(true);
	});

	SECP_TEST_CASES.forEach((address, base64) => {
		it(`toSuiAddress from base64 public key ${address}`, () => {
			const key = new Secp256k1PublicKey(base64);
			expect(key.toSuiAddress()).toEqual(address);
		});
	});
});
