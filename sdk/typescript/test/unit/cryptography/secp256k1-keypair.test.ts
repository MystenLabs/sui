// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64, toB58, toB64 } from '@mysten/bcs';
import { secp256k1 } from '@noble/curves/secp256k1';
import { sha256 } from '@noble/hashes/sha256';
import { describe, expect, it } from 'vitest';

import { decodeSuiPrivateKey } from '../../../src/cryptography/keypair';
import {
	DEFAULT_SECP256K1_DERIVATION_PATH,
	Secp256k1Keypair,
} from '../../../src/keypairs/secp256k1';
import { Transaction } from '../../../src/transactions';
import { verifyPersonalMessageSignature, verifyTransactionSignature } from '../../../src/verify';

const PRIVATE_KEY_SIZE = 32;

// Test case from https://github.com/rust-bitcoin/rust-secp256k1/blob/master/examples/sign_verify.rs#L26
const VALID_SECP256K1_SECRET_KEY = [
	59, 148, 11, 85, 134, 130, 61, 253, 2, 174, 59, 70, 27, 180, 51, 107, 94, 203, 174, 253, 102, 39,
	170, 146, 46, 252, 4, 143, 236, 12, 136, 28,
];

// Corresponding to the secret key above.
export const VALID_SECP256K1_PUBLIC_KEY = [
	2, 29, 21, 35, 7, 198, 183, 43, 14, 208, 65, 139, 14, 112, 205, 128, 231, 245, 41, 91, 141, 134,
	245, 114, 45, 63, 82, 19, 251, 210, 57, 79, 54,
];

// Invalid private key with incorrect length
export const INVALID_SECP256K1_SECRET_KEY = Uint8Array.from(Array(PRIVATE_KEY_SIZE - 1).fill(1));

// Invalid public key with incorrect length
export const INVALID_SECP256K1_PUBLIC_KEY = Uint8Array.from(Array(PRIVATE_KEY_SIZE).fill(1));

// Test case generated against rust keytool cli. See https://github.com/MystenLabs/sui/blob/edd2cd31e0b05d336b1b03b6e79a67d8dd00d06b/crates/sui/src/unit_tests/keytool_tests.rs#L165
const TEST_CASES = [
	[
		'film crazy soon outside stand loop subway crumble thrive popular green nuclear struggle pistol arm wife phrase warfare march wheat nephew ask sunny firm',
		'suiprivkey1qyqr6yvxdqkh32ep4pk9caqvphmk9epn6rhkczcrhaeermsyvwsg783y9am',
		'0x9e8f732575cc5386f8df3c784cd3ed1b53ce538da79926b2ad54dcc1197d2532',
	],
	[
		'require decline left thought grid priority false tiny gasp angle royal system attack beef setup reward aunt skill wasp tray vital bounce inflict level',
		'suiprivkey1q8hexn5m2u36tx39ln5e22hfseadknp7d2qlkhe30ejy7fc6am5aqkqpqsj',
		'0x9fd5a804ed6b46d36949ff7434247f0fd594673973ece24aede6b86a7b5dae01',
	],
	[
		'organ crash swim stick traffic remember army arctic mesh slice swear summer police vast chaos cradle squirrel hood useless evidence pet hub soap lake',
		'suiprivkey1qxx6yf53jgxvsmccst8cuwnj0rx4k4uzvn9aalvag7ns0xf0g8j2x246jst',
		'0x60287d7c38dee783c2ab1077216124011774be6b0764d62bd05f32c88979d5c5',
	],
];

const TEST_MNEMONIC =
	'result crisp session latin must fruit genuine question prevent start coconut brave speak student dismiss';

describe('secp256k1-keypair', () => {
	it('new keypair', () => {
		const keypair = new Secp256k1Keypair();
		expect(keypair.getPublicKey().toRawBytes().length).toBe(33);
		expect(2).toEqual(2);
	});

	it('create keypair from secret key', () => {
		const secret_key = new Uint8Array(VALID_SECP256K1_SECRET_KEY);
		const pub_key = new Uint8Array(VALID_SECP256K1_PUBLIC_KEY);
		let pub_key_base64 = toB64(pub_key);
		const keypair = Secp256k1Keypair.fromSecretKey(secret_key);
		expect(keypair.getPublicKey().toRawBytes()).toEqual(new Uint8Array(pub_key));
		expect(keypair.getPublicKey().toBase64()).toEqual(pub_key_base64);
	});

	it('creating keypair from invalid secret key throws error', () => {
		const secret_key = new Uint8Array(INVALID_SECP256K1_SECRET_KEY);
		let secret_key_base64 = toB64(secret_key);
		const secretKey = fromB64(secret_key_base64);
		expect(() => {
			Secp256k1Keypair.fromSecretKey(secretKey);
		}).toThrow('private key must be 32 bytes, hex or bigint, not object');
	});

	it('generate keypair from random seed', () => {
		const keypair = Secp256k1Keypair.fromSeed(Uint8Array.from(Array(PRIVATE_KEY_SIZE).fill(8)));
		expect(keypair.getPublicKey().toBase64()).toEqual(
			'A/mR+UTR4ZVKf8i5v2Lg148BX0wHdi1QXiDmxFJgo2Yb',
		);
	});

	it('signature of data is valid', async () => {
		const keypair = new Secp256k1Keypair();
		const signData = new TextEncoder().encode('hello world');

		const msgHash = sha256(signData);
		const sig = await keypair.sign(signData);
		expect(
			secp256k1.verify(
				secp256k1.Signature.fromCompact(sig),
				msgHash,
				keypair.getPublicKey().toRawBytes(),
			),
		).toBeTruthy();
	});

	it('signature of data is same as rust implementation', async () => {
		const secret_key = new Uint8Array(VALID_SECP256K1_SECRET_KEY);
		const keypair = Secp256k1Keypair.fromSecretKey(secret_key);
		const signData = new TextEncoder().encode('Hello, world!');

		const msgHash = sha256(signData);
		const sig = await keypair.sign(signData);

		// Assert the signature is the same as the rust implementation. See https://github.com/MystenLabs/fastcrypto/blob/0436d6ef11684c291b75c930035cb24abbaf581e/fastcrypto/src/tests/secp256k1_tests.rs#L115
		expect(Buffer.from(sig).toString('hex')).toEqual(
			'25d450f191f6d844bf5760c5c7b94bc67acc88be76398129d7f43abdef32dc7f7f1a65b7d65991347650f3dd3fa3b3a7f9892a0608521cbcf811ded433b31f8b',
		);
		expect(
			secp256k1.verify(
				secp256k1.Signature.fromCompact(sig),
				msgHash,
				keypair.getPublicKey().toRawBytes(),
			),
		).toBeTruthy();
	});

	it('invalid mnemonics to derive secp256k1 keypair', () => {
		expect(() => {
			Secp256k1Keypair.deriveKeypair('aaa', DEFAULT_SECP256K1_DERIVATION_PATH);
		}).toThrow('Invalid mnemonic');
	});

	it('create keypair from secret key and mnemonics matches keytool', () => {
		for (const t of TEST_CASES) {
			// Keypair derived from mnemonic
			const keypair = Secp256k1Keypair.deriveKeypair(t[0]);
			expect(keypair.getPublicKey().toSuiAddress()).toEqual(t[2]);

			// Keypair derived from Bech32 string.
			const parsed = decodeSuiPrivateKey(t[1]);
			const kp = Secp256k1Keypair.fromSecretKey(parsed.secretKey);
			expect(kp.getPublicKey().toSuiAddress()).toEqual(t[2]);

			// Exported keypair matches the Bech32 encoded secret key.
			const exported = kp.getSecretKey();
			expect(exported).toEqual(t[1]);
		}
	});

	it('incorrect purpose node for secp256k1 derivation path', () => {
		expect(() => {
			Secp256k1Keypair.deriveKeypair(TEST_MNEMONIC, `m/44'/784'/0'/0'/0'`);
		}).toThrow('Invalid derivation path');
	});

	it('incorrect hardened path for secp256k1 key derivation', () => {
		expect(() => {
			Secp256k1Keypair.deriveKeypair(TEST_MNEMONIC, `m/54'/784'/0'/0'/0'`);
		}).toThrow('Invalid derivation path');
	});

	it('signs Transactions', async () => {
		const keypair = new Secp256k1Keypair();
		const tx = new Transaction();
		tx.setSender(keypair.getPublicKey().toSuiAddress());
		tx.setGasPrice(5);
		tx.setGasBudget(100);
		tx.setGasPayment([
			{
				objectId: (Math.random() * 100000).toFixed(0).padEnd(64, '0'),
				version: String((Math.random() * 10000).toFixed(0)),
				digest: toB58(
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
		expect(await keypair.getPublicKey().verifyTransaction(bytes, serializedSignature)).toEqual(
			true,
		);
		expect(!!(await verifyTransactionSignature(bytes, serializedSignature))).toEqual(true);
	});

	it('signs PersonalMessages', async () => {
		const keypair = new Secp256k1Keypair();
		const message = new TextEncoder().encode('hello world');

		const serializedSignature = (await keypair.signPersonalMessage(message)).signature;

		expect(
			await keypair.getPublicKey().verifyPersonalMessage(message, serializedSignature),
		).toEqual(true);
		expect(
			await keypair.getPublicKey().verifyPersonalMessage(message, serializedSignature),
		).toEqual(true);
		expect(!!(await verifyPersonalMessageSignature(message, serializedSignature))).toEqual(true);
	});
});
