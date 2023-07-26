// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64, toB58, toB64 } from '@mysten/bcs';
import nacl from 'tweetnacl';
import { describe, it, expect } from 'vitest';
import { TransactionBlock } from '../../../src/builder';
import { Ed25519Keypair } from '../../../src/keypairs/ed25519';
import { verifyPersonalMessage, verifyTransactionBlock } from '../../../src/verify';

const VALID_SECRET_KEY = 'mdqVWeFekT7pqy5T49+tV12jO0m+ESW7ki4zSU9JiCg=';
const PRIVATE_KEY_SIZE = 32;

// Test case generated against rust keytool cli. See https://github.com/MystenLabs/sui/blob/edd2cd31e0b05d336b1b03b6e79a67d8dd00d06b/crates/sui/src/unit_tests/keytool_tests.rs#L165
const TEST_CASES = [
	[
		'film crazy soon outside stand loop subway crumble thrive popular green nuclear struggle pistol arm wife phrase warfare march wheat nephew ask sunny firm',
		'AN0JMHpDum3BhrVwnkylH0/HGRHBQ/fO/8+MYOawO8j6',
		'0xa2d14fad60c56049ecf75246a481934691214ce413e6a8ae2fe6834c173a6133',
	],
	[
		'require decline left thought grid priority false tiny gasp angle royal system attack beef setup reward aunt skill wasp tray vital bounce inflict level',
		'AJrA997C1eVz6wYIp7bO8dpITSRBXpvg1m70/P3gusu2',
		'0x1ada6e6f3f3e4055096f606c746690f1108fcc2ca479055cc434a3e1d3f758aa',
	],
	[
		'organ crash swim stick traffic remember army arctic mesh slice swear summer police vast chaos cradle squirrel hood useless evidence pet hub soap lake',
		'AAEMSIQeqyz09StSwuOW4MElQcZ+4jHW4/QcWlJEf5Yk',
		'0xe69e896ca10f5a77732769803cc2b5707f0ab9d4407afb5e4b4464b89769af14',
	],
];

const TEST_MNEMONIC =
	'result crisp session latin must fruit genuine question prevent start coconut brave speak student dismiss';

describe('ed25519-keypair', () => {
	it('new keypair', () => {
		const keypair = new Ed25519Keypair();
		expect(keypair.getPublicKey().toRawBytes().length).toBe(32);
		expect(2).toEqual(2);
	});

	it('create keypair from secret key', () => {
		const secretKey = fromB64(VALID_SECRET_KEY);
		const keypair = Ed25519Keypair.fromSecretKey(secretKey);
		expect(keypair.getPublicKey().toBase64()).toEqual(
			'Gy9JCW4+Xb0Pz6nAwM2S2as7IVRLNNXdSmXZi4eLmSI=',
		);
	});

	it('create keypair from secret key and mnemonics matches keytool', () => {
		for (const t of TEST_CASES) {
			// Keypair derived from mnemonic
			const keypair = Ed25519Keypair.deriveKeypair(t[0]);
			expect(keypair.getPublicKey().toSuiAddress()).toEqual(t[2]);

			// Keypair derived from 32-byte secret key
			const raw = fromB64(t[1]);
			if (raw[0] !== 0 || raw.length !== PRIVATE_KEY_SIZE + 1) {
				throw new Error('invalid key');
			}
			const imported = Ed25519Keypair.fromSecretKey(raw.slice(1));
			expect(imported.getPublicKey().toSuiAddress()).toEqual(t[2]);

			// Exported secret key matches the 32-byte secret key.
			const exported = imported.export();
			expect(exported.privateKey).toEqual(toB64(raw.slice(1)));
		}
	});

	it('generate keypair from random seed', () => {
		const keypair = Ed25519Keypair.fromSecretKey(Uint8Array.from(Array(PRIVATE_KEY_SIZE).fill(8)));
		expect(keypair.getPublicKey().toBase64()).toEqual(
			'E5j2LG0aRXxRumpLXz29L2n8qTIWIY3ImX5Ba9F9k8o=',
		);
	});

	it('signature of data is valid', () => {
		const keypair = new Ed25519Keypair();
		const signData = new TextEncoder().encode('hello world');
		const signature = keypair.signData(signData);
		const isValid = nacl.sign.detached.verify(
			signData,
			signature,
			keypair.getPublicKey().toRawBytes(),
		);
		expect(isValid).toBeTruthy();
		expect(keypair.getPublicKey().verify(signData, signature));
	});

	it('incorrect coin type node for ed25519 derivation path', () => {
		const keypair = Ed25519Keypair.deriveKeypair(TEST_MNEMONIC, `m/44'/784'/0'/0'/0'`);

		const signData = new TextEncoder().encode('hello world');
		const signature = keypair.signData(signData);
		const isValid = nacl.sign.detached.verify(
			signData,
			signature,
			keypair.getPublicKey().toRawBytes(),
		);
		expect(isValid).toBeTruthy();
	});

	it('incorrect coin type node for ed25519 derivation path', () => {
		expect(() => {
			Ed25519Keypair.deriveKeypair(TEST_MNEMONIC, `m/44'/0'/0'/0'/0'`);
		}).toThrow('Invalid derivation path');
	});

	it('incorrect purpose node for ed25519 derivation path', () => {
		expect(() => {
			Ed25519Keypair.deriveKeypair(TEST_MNEMONIC, `m/54'/784'/0'/0'/0'`);
		}).toThrow('Invalid derivation path');
	});

	it('invalid mnemonics to derive ed25519 keypair', () => {
		expect(() => {
			Ed25519Keypair.deriveKeypair('aaa');
		}).toThrow('Invalid mnemonic');
	});

	it('signs TransactionBlocks', async () => {
		const keypair = new Ed25519Keypair();
		const txb = new TransactionBlock();
		txb.setSender(keypair.getPublicKey().toSuiAddress());
		txb.setGasPrice(5);
		txb.setGasBudget(100);
		txb.setGasPayment([
			{
				objectId: (Math.random() * 100000).toFixed(0).padEnd(64, '0'),
				version: String((Math.random() * 10000).toFixed(0)),
				digest: toB58(new Uint8Array([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9])),
			},
		]);

		const bytes = await txb.build();

		const serializedSignature = (await keypair.signTransactionBlock(bytes)).signature;

		expect(await keypair.getPublicKey().verifyTransactionBlock(bytes, serializedSignature)).toEqual(
			true,
		);
		expect(await keypair.getPublicKey().verifyTransactionBlock(bytes, serializedSignature)).toEqual(
			true,
		);
		expect(!!(await verifyTransactionBlock(bytes, serializedSignature))).toEqual(true);
	});

	it('signs PersonalMessages', async () => {
		const keypair = new Ed25519Keypair();
		const message = new TextEncoder().encode('hello world');

		const serializedSignature = (await keypair.signPersonalMessage(message)).signature;

		expect(
			await keypair.getPublicKey().verifyPersonalMessage(message, serializedSignature),
		).toEqual(true);
		expect(
			await keypair.getPublicKey().verifyPersonalMessage(message, serializedSignature),
		).toEqual(true);
		expect(!!(await verifyPersonalMessage(message, serializedSignature))).toEqual(true);
	});
});
