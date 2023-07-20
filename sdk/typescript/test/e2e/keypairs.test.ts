// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';
import { fromB64, toB64 } from '../../src';
import { IntentScope, messageWithIntent, parseSerializedSignature } from '../../src/cryptography';
import { Ed25519Keypair } from '../../src/keypairs/ed25519';
import { Secp256k1Keypair } from '../../src/keypairs/secp256k1';

import { blake2b } from '@noble/hashes/blake2b';

const TX_BYTES =
	'AAACAQDMdYtdFSLGe6VbgpuIsMksv9Ypzpvkq2jiYq0hAjUpOQIAAAAAAAAAIHGwPza+lUm6RuJV1vn9pA4y0PwVT7k/KMMbUViQS5ydACAMVn/9+BYsttUa90vgGZRDuS6CPUumztJN5cbEY3l9RgEBAQEAAAEBAHUFfdk1Tg9l6STLBoSBJbbUuehTDUlLH7p81kpqCKsaBCiJ034Ac84f1oqgmpz79O8L/UeLNDUpOUMa+LadeX93AgAAAAAAAAAgs1e67e789jSlrzOJUXq0bb7Bn/hji+3F5UoMAbze595xCSZCVjU1ItUC9G7KQjygNiBbzZe8t7YLPjRAQyGTzAIAAAAAAAAAIAujHFcrkJJhZfCmxmCHsBWxj5xkviUqB479oupdgMZu07b+hkrjyvCcX50dO30v3PszXFj7+lCNTUTuE4UI3eoCAAAAAAAAACBIv39dyVELUFTkNv72mat5R1uHFkQdViikc1lTMiSVlOD+eESUq3neyciBatafk9dHuhhrS37RaSflqKwFlwzPAgAAAAAAAAAg8gqL3hCkAho8bb0PoqshJdqQFoRP8ZmQMZDFvsGBqa11BX3ZNU4PZekkywaEgSW21LnoUw1JSx+6fNZKagirGgEAAAAAAAAAKgQAAAAAAAAA';
const DIGEST = 'VMVv+/L/EG7/yhEbCQ1qiSt30JXV8yIm+4xO6yTkqeM=';
const DERIVATION_PATH = `m/44'/784'/0'/0'/0'`;
const DERIVATION_PATH_SECP256K1 = `m/54'/784'/0'/0/0`;

// Test cases for Ed25519.
// First element is the mnemonics, second element is the
// base64 encoded pubkey, derived using DERIVATION_PATH,
// third element is the hex encoded address, fourth
// element is the valid signature produced for TX_BYTES.
const TEST_CASES = [
	[
		'film crazy soon outside stand loop subway crumble thrive popular green nuclear struggle pistol arm wife phrase warfare march wheat nephew ask sunny firm',
		'ImR/7u82MGC9QgWhZxoV8QoSNnZZGLG19jjYLzPPxGk=',
		'0xa2d14fad60c56049ecf75246a481934691214ce413e6a8ae2fe6834c173a6133',
		'NwIObhuKot7QRWJu4wWCC5ttOgEfN7BrrVq1draImpDZqtKEaWjNNRKKfWr1FL4asxkBlQd8IwpxpKSTzcXMAQ==',
	],
	[
		'require decline left thought grid priority false tiny gasp angle royal system attack beef setup reward aunt skill wasp tray vital bounce inflict level',
		'vG6hEnkYNIpdmWa/WaLivd1FWBkxG+HfhXkyWgs9uP4=',
		'0x1ada6e6f3f3e4055096f606c746690f1108fcc2ca479055cc434a3e1d3f758aa',
		'8BSMw/VdYSXxbpl5pp8b5ylWLntTWfBG3lSvAHZbsV9uD2/YgsZDbhVba4rIPhGTn3YvDNs3FOX5+EIXMup3Bw==',
	],
	[
		'organ crash swim stick traffic remember army arctic mesh slice swear summer police vast chaos cradle squirrel hood useless evidence pet hub soap lake',
		'arEzeF7Uu90jP4Sd+Or17c+A9kYviJpCEQAbEt0FHbU=',
		'0xe69e896ca10f5a77732769803cc2b5707f0ab9d4407afb5e4b4464b89769af14',
		'/ihBMku1SsqK+yDxNY47N/tAREZ+gWVTvZrUoCHsGGR9CoH6E7SLKDRYY9RnwBw/Bt3wWcdJ0Wc2Q3ioHIlzDA==',
	],
];

// Test cases for Secp256k1.
// First element is the mnemonics, second element is the
// base64 encoded pubkey, derived using DERIVATION_PATH_SECP256K1,
// third element is the hex encoded address, fourth
// element is the valid signature produced for TX_BYTES.
const TEST_CASES_SECP256K1 = [
	[
		'film crazy soon outside stand loop subway crumble thrive popular green nuclear struggle pistol arm wife phrase warfare march wheat nephew ask sunny firm',
		'Ar2Vs2ei2HgaCIvcsAVAZ6bKYXhDfRTlF432p8Wn4lsL',
		'0x9e8f732575cc5386f8df3c784cd3ed1b53ce538da79926b2ad54dcc1197d2532',
		'y7a8KDd9Py4i5GIka7zAFSHOCOTVLm9ibDx3wPd6WsQa7C1FUdxz32+h5TYmNNWUpTRgWgdBAeG9OgAnDBg0cQ==',
	],
	[
		'require decline left thought grid priority false tiny gasp angle royal system attack beef setup reward aunt skill wasp tray vital bounce inflict level',
		'A5IcrmWDxl0J/4MNkrtE1AvwiLZiqih9tjttcGlafw+m',
		'0x9fd5a804ed6b46d36949ff7434247f0fd594673973ece24aede6b86a7b5dae01',
		'ijfLeBFowLQjuUD9h6/q9cmuZGg1Afo0ZgZJsZrJsIt2FSFddIomOzqnan3v1T9aW14aBMAlymUELgryib4Q/A==',
	],
	[
		'organ crash swim stick traffic remember army arctic mesh slice swear summer police vast chaos cradle squirrel hood useless evidence pet hub soap lake',
		'AuEiECTZwyHhqStzpO/RNBXO89/Wa8oc4BtoneKnl6h8',
		'0x60287d7c38dee783c2ab1077216124011774be6b0764d62bd05f32c88979d5c5',
		'qAZyBPNUOtr2uKNByx9HNpWlDxQrOCjtMajYXGTAtWMnTp1cFYMe6QyT0EsYJ0t5xKtK8xq29yChbpsHWz94qA==',
	],
];

describe('Keypairs', () => {
	it('Ed25519 keypair signData', async () => {
		const tx_bytes = fromB64(TX_BYTES);
		const intentMessage = messageWithIntent(IntentScope.TransactionData, tx_bytes);
		const digest = blake2b(intentMessage, { dkLen: 32 });
		expect(toB64(digest)).toEqual(DIGEST);

		for (const t of TEST_CASES) {
			const keypair = Ed25519Keypair.deriveKeypair(t[0], DERIVATION_PATH);
			expect(keypair.getPublicKey().toBase64()).toEqual(t[1]);
			expect(keypair.getPublicKey().toSuiAddress()).toEqual(t[2]);

			const { signature: serializedSignature } = await keypair.signTransactionBlock(tx_bytes);
			const { signature } = parseSerializedSignature(serializedSignature);

			expect(toB64(signature!)).toEqual(t[3]);

			const isValid = await keypair
				.getPublicKey()
				.verifyTransactionBlock(tx_bytes, serializedSignature);
			expect(isValid).toBeTruthy();
		}
	});

	it('Ed25519 keypair signMessage', async () => {
		const keypair = new Ed25519Keypair();
		const signData = new TextEncoder().encode('hello world');

		const { signature } = await keypair.signPersonalMessage(signData);
		const isValid = await keypair.getPublicKey().verifyPersonalMessage(signData, signature);
		expect(isValid).toBe(true);
	});

	it('Ed25519 keypair invalid signMessage', async () => {
		const keypair = new Ed25519Keypair();
		const signData = new TextEncoder().encode('hello world');

		const { signature } = await keypair.signPersonalMessage(signData);
		const isValid = await keypair
			.getPublicKey()
			.verifyPersonalMessage(new TextEncoder().encode('hello worlds'), signature);
		expect(isValid).toBe(false);
	});

	it('Secp256k1 keypair signData', async () => {
		const tx_bytes = fromB64(TX_BYTES);
		const intentMessage = messageWithIntent(IntentScope.TransactionData, tx_bytes);
		const digest = blake2b(intentMessage, { dkLen: 32 });
		expect(toB64(digest)).toEqual(DIGEST);

		for (const t of TEST_CASES_SECP256K1) {
			const keypair = Secp256k1Keypair.deriveKeypair(t[0], DERIVATION_PATH_SECP256K1);
			expect(keypair.getPublicKey().toBase64()).toEqual(t[1]);
			expect(keypair.getPublicKey().toSuiAddress()).toEqual(t[2]);

			const { signature: serializedSignature } = await keypair.signTransactionBlock(tx_bytes);
			const { signature } = parseSerializedSignature(serializedSignature);

			expect(toB64(signature!)).toEqual(t[3]);

			const isValid = await keypair
				.getPublicKey()
				.verifyTransactionBlock(tx_bytes, serializedSignature);
			expect(isValid).toBeTruthy();
		}
	});

	it('Secp256k1 keypair signMessage', async () => {
		const keypair = new Secp256k1Keypair();
		const signData = new TextEncoder().encode('hello world');

		const { signature } = await keypair.signPersonalMessage(signData);

		const isValid = await keypair.getPublicKey().verifyPersonalMessage(signData, signature);
		expect(isValid).toBe(true);
	});
});
