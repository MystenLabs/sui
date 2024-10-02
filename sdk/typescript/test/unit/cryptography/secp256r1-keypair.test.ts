// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromBase64, toBase58, toBase64 } from '@mysten/bcs';
import { secp256r1 } from '@noble/curves/p256';
import { sha256 } from '@noble/hashes/sha256';
import { describe, expect, it } from 'vitest';

import { decodeSuiPrivateKey } from '../../../src/cryptography/keypair';
import {
	DEFAULT_SECP256R1_DERIVATION_PATH,
	Secp256r1Keypair,
} from '../../../src/keypairs/secp256r1';
import { Transaction } from '../../../src/transactions';
import { verifyPersonalMessageSignature, verifyTransactionSignature } from '../../../src/verify';

const VALID_SECP256R1_SECRET_KEY = [
	66, 37, 141, 205, 161, 76, 241, 17, 198, 2, 184, 151, 27, 140, 200, 67, 233, 30, 70, 202, 144, 81,
	81, 192, 39, 68, 166, 176, 23, 230, 147, 22,
];

// Corresponding to the secret key above.
export const VALID_SECP256R1_PUBLIC_KEY = [
	2, 39, 50, 43, 58, 137, 26, 10, 40, 13, 107, 193, 251, 44, 187, 35, 210, 143, 84, 144, 111, 214,
	64, 127, 95, 116, 31, 109, 239, 87, 98, 96, 154,
];

const PRIVATE_KEY_SIZE = 32;
// Invalid private key with incorrect length
export const INVALID_SECP256R1_SECRET_KEY = Uint8Array.from(Array(PRIVATE_KEY_SIZE - 1).fill(1));

// Invalid public key with incorrect length
export const INVALID_SECP256R1_PUBLIC_KEY = Uint8Array.from(Array(PRIVATE_KEY_SIZE).fill(1));

// Test case generated against rust keytool cli. See https://github.com/MystenLabs/sui/blob/edd2cd31e0b05d336b1b03b6e79a67d8dd00d06b/crates/sui/src/unit_tests/keytool_tests.rs#L165
const TEST_CASES = [
	[
		'act wing dilemma glory episode region allow mad tourist humble muffin oblige',
		'suiprivkey1qgj6vet4rstf2p00j860xctkg4fyqqq5hxgu4mm0eg60fq787ujnqs5wc8q',
		'0x4a822457f1970468d38dae8e63fb60eefdaa497d74d781f581ea2d137ec36f3a',
	],
	[
		'flag rebel cabbage captain minimum purpose long already valley horn enrich salt',
		'suiprivkey1qgmgr6dza8slgxn0rcxcy47xeas9l565cc5q440ngdzr575rc2356gzlq7a',
		'0xcd43ecb9dd32249ff5748f5e4d51855b01c9b1b8bbe7f8638bb8ab4cb463b920',
	],
	[
		'area renew bar language pudding trial small host remind supreme cabbage era',
		'suiprivkey1qt2gsye4dyn0lxey0ht6d5f2ada7ew9044a49y2f3mymy2uf0hr55jmfze3',
		'0x0d9047b7e7b698cc09c955ea97b0c68c2be7fb3aebeb59edcc84b1fb87e0f28e',
	],
];

describe('secp256r1-keypair', () => {
	it('new keypair', () => {
		const keypair = new Secp256r1Keypair();
		expect(keypair.getPublicKey().toRawBytes().length).toBe(33);
		expect(2).toEqual(2);
	});

	it('create keypair from secret key', () => {
		const secret_key = new Uint8Array(VALID_SECP256R1_SECRET_KEY);
		const pub_key = new Uint8Array(VALID_SECP256R1_PUBLIC_KEY);
		let pub_key_base64 = toBase64(pub_key);
		const keypair = Secp256r1Keypair.fromSecretKey(secret_key);
		expect(keypair.getPublicKey().toRawBytes()).toEqual(new Uint8Array(pub_key));
		expect(keypair.getPublicKey().toBase64()).toEqual(pub_key_base64);
	});

	it('creating keypair from invalid secret key throws error', () => {
		const secret_key = new Uint8Array(INVALID_SECP256R1_SECRET_KEY);
		let secret_key_base64 = toBase64(secret_key);
		const secretKey = fromBase64(secret_key_base64);
		expect(() => {
			Secp256r1Keypair.fromSecretKey(secretKey);
		}).toThrow('private key must be 32 bytes, hex or bigint, not object');
	});

	it('generate keypair from random seed', () => {
		const keypair = Secp256r1Keypair.fromSeed(Uint8Array.from(Array(PRIVATE_KEY_SIZE).fill(8)));
		expect(keypair.getPublicKey().toBase64()).toEqual(
			'AzrasV1mJWvxXNcWA1s/BBRE5RL+0d1k1Lp1WX0g42bx',
		);
	});

	it('signature of data is valid', async () => {
		const keypair = new Secp256r1Keypair();
		const signData = new TextEncoder().encode('hello world');

		const msgHash = sha256(signData);
		const sig = await keypair.sign(signData);
		expect(
			secp256r1.verify(
				secp256r1.Signature.fromCompact(sig),
				msgHash,
				keypair.getPublicKey().toRawBytes(),
			),
		).toBeTruthy();
	});

	it('signature of data is same as rust implementation', async () => {
		const secret_key = new Uint8Array(VALID_SECP256R1_SECRET_KEY);
		const keypair = Secp256r1Keypair.fromSecretKey(secret_key);
		const signData = new TextEncoder().encode('Hello, world!');

		const msgHash = sha256(signData);
		const sig = await keypair.sign(signData);

		// Assert the signature is the same as the rust implementation.
		expect(Buffer.from(sig).toString('hex')).toEqual(
			'26d84720652d8bc4ddd1986434a10b3b7b69f0e35a17c6a5987e6d1cba69652f4384a342487642df5e44592d304bea0ceb0fae2e347fa3cec5ce1a8144cfbbb2',
		);
		expect(
			secp256r1.verify(
				secp256r1.Signature.fromCompact(sig),
				msgHash,
				keypair.getPublicKey().toRawBytes(),
			),
		).toBeTruthy();
	});

	it('invalid mnemonics to derive secp256r1 keypair', () => {
		expect(() => {
			Secp256r1Keypair.deriveKeypair('aaa', DEFAULT_SECP256R1_DERIVATION_PATH);
		}).toThrow('Invalid mnemonic');
	});

	it('create keypair from secret key and mnemonics matches keytool', () => {
		for (const t of TEST_CASES) {
			// Keypair derived from mnemonic
			const keypair = Secp256r1Keypair.deriveKeypair(t[0]);
			expect(keypair.getPublicKey().toSuiAddress()).toEqual(t[2]);

			// Keypair derived from Bech32 string.
			const parsed = decodeSuiPrivateKey(t[1]);
			const kp = Secp256r1Keypair.fromSecretKey(parsed.secretKey);
			expect(kp.getPublicKey().toSuiAddress()).toEqual(t[2]);

			// Exported keypair matches the Bech32 encoded secret key.
			const exported = kp.getSecretKey();
			expect(exported).toEqual(t[1]);
		}
	});

	it('incorrect purpose node for secp256r1 derivation path', () => {
		expect(() => {
			Secp256r1Keypair.deriveKeypair(TEST_CASES[0][0], `m/54'/784'/0'/0'/0'`);
		}).toThrow('Invalid derivation path');
	});

	it('incorrect hardened path for secp256k1 key derivation', () => {
		expect(() => {
			Secp256r1Keypair.deriveKeypair(TEST_CASES[0][0], `m/44'/784'/0'/0'/0'`);
		}).toThrow('Invalid derivation path');
	});

	it('signs Transactions', async () => {
		const keypair = new Secp256r1Keypair();
		const tx = new Transaction();
		tx.setSender(keypair.getPublicKey().toSuiAddress());
		tx.setGasPrice(5);
		tx.setGasBudget(100);
		tx.setGasPayment([
			{
				objectId: (Math.random() * 100000).toFixed(0).padEnd(64, '0'),
				version: String((Math.random() * 10000).toFixed(0)),
				digest: toBase58(
					new Uint8Array([
						0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8,
						9, 1, 2,
					]),
				),
			},
		]);

		const bytes = await tx.build();

		const serializedSignature = (await keypair.signTransaction(bytes)).signature;

		expect(await keypair.getPublicKey().verifyTransaction(bytes, serializedSignature)).toEqual(
			true,
		);
		expect(!!(await verifyTransactionSignature(bytes, serializedSignature))).toEqual(true);
	});

	it('signs PersonalMessages', async () => {
		const keypair = new Secp256r1Keypair();
		const message = new TextEncoder().encode('hello world');

		const serializedSignature = (await keypair.signPersonalMessage(message)).signature;

		expect(
			await keypair.getPublicKey().verifyPersonalMessage(message, serializedSignature),
		).toEqual(true);
		expect(!!(await verifyPersonalMessageSignature(message, serializedSignature))).toEqual(true);
	});
});
