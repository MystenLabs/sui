// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromBase64, toBase58, toBase64 } from '@mysten/bcs';
import { beforeAll, describe, expect, it, test } from 'vitest';

import { bcs } from '../../../src/bcs';
import { parseSerializedSignature, SIGNATURE_SCHEME_TO_FLAG } from '../../../src/cryptography';
import { SignatureWithBytes } from '../../../src/cryptography/keypair';
import { PublicKey } from '../../../src/cryptography/publickey';
import { Ed25519Keypair, Ed25519PublicKey } from '../../../src/keypairs/ed25519';
import { Secp256k1Keypair } from '../../../src/keypairs/secp256k1';
import { Secp256r1Keypair } from '../../../src/keypairs/secp256r1';
import { MultiSigPublicKey, MultiSigSigner, parsePartialSignatures } from '../../../src/multisig';
import { Transaction } from '../../../src/transactions';
import { verifyPersonalMessageSignature, verifyTransactionSignature } from '../../../src/verify';
import { toZkLoginPublicIdentifier } from '../../../src/zklogin/publickey';

describe('Multisig scenarios', () => {
	it('multisig address creation and combine sigs using Secp256r1Keypair', async () => {
		const k1 = new Ed25519Keypair();
		const pk1 = k1.getPublicKey();

		const k2 = new Secp256k1Keypair();
		const pk2 = k2.getPublicKey();

		const k3 = new Secp256r1Keypair();
		const pk3 = k3.getPublicKey();

		const pubkeyWeightPairs = [
			{
				publicKey: pk1,
				weight: 1,
			},
			{
				publicKey: pk2,
				weight: 2,
			},
			{
				publicKey: pk3,
				weight: 3,
			},
		];

		const tx = new Transaction();
		tx.setSender(k3.getPublicKey().toSuiAddress());
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

		const { signature } = await k3.signTransaction(bytes);

		const publicKey = MultiSigPublicKey.fromPublicKeys({
			threshold: 3,
			publicKeys: pubkeyWeightPairs,
		});

		const multisig = publicKey.combinePartialSignatures([signature]);

		expect(await k3.getPublicKey().verifyTransaction(bytes, signature)).toEqual(true);

		const parsed = parseSerializedSignature(multisig);
		if (parsed.signatureScheme !== 'MultiSig') {
			throw new Error('Expected signature scheme to be MultiSig');
		}
		const publicKey2 = new MultiSigPublicKey(parsed.multisig!.multisig_pk);

		// multisig (sig3 weight 3 >= threshold ) verifies ok
		expect(await publicKey2.verifyTransaction(bytes, multisig)).toEqual(true);
	});

	it('providing false number of signatures to combining via different methods', async () => {
		const k1 = new Ed25519Keypair();
		const pk1 = k1.getPublicKey();

		const k2 = new Secp256k1Keypair();
		const pk2 = k2.getPublicKey();

		const k3 = new Secp256r1Keypair();

		const multiSigPublicKey = MultiSigPublicKey.fromPublicKeys({
			threshold: 3,
			publicKeys: [
				{ publicKey: pk1, weight: 1 },
				{ publicKey: pk2, weight: 2 },
			],
		});

		const signData = new TextEncoder().encode('hello world');
		const sig1 = await k1.signPersonalMessage(signData);
		const sig2 = await k2.signPersonalMessage(signData);
		const sig3 = await k3.signPersonalMessage(signData);

		const isValidSig1 = await k1.getPublicKey().verifyPersonalMessage(signData, sig1.signature);
		const isValidSig2 = await k2.getPublicKey().verifyPersonalMessage(signData, sig2.signature);

		expect(isValidSig1).toBe(true);
		expect(isValidSig2).toBe(true);

		// create invalid signature

		const compressedSignatures: ({ ED25519: number[] } | { Secp256r1: number[] })[] = [
			{
				ED25519: Array.from(
					parseSerializedSignature(sig1.signature).signature!.map((x: number) => Number(x)),
				),
			},
			{
				Secp256r1: Array.from(
					parseSerializedSignature(sig1.signature).signature!.map((x: number) => Number(x)),
				),
			},
		];

		const bytes = bcs.MultiSig.serialize({
			sigs: compressedSignatures,
			bitmap: 5,
			multisig_pk: bcs.MultiSigPublicKey.parse(multiSigPublicKey.toRawBytes()),
		}).toBytes();
		let tmp = new Uint8Array(bytes.length + 1);
		tmp.set([SIGNATURE_SCHEME_TO_FLAG['MultiSig']]);
		tmp.set(bytes, 1);

		const multisig = toBase64(tmp);

		expect(() =>
			multiSigPublicKey.combinePartialSignatures([sig1.signature, sig3.signature]),
		).toThrowError(new Error('Received signature from unknown public key'));

		const parsed = parseSerializedSignature(multisig);
		if (parsed.signatureScheme !== 'MultiSig') {
			throw new Error('Expected signature scheme to be MultiSig');
		}
		const publicKey = new MultiSigPublicKey(parsed.multisig!.multisig_pk);

		await expect(publicKey.verifyPersonalMessage(signData, multisig)).rejects.toThrow(
			new Error("Cannot read properties of undefined (reading 'pubKey')"),
		);
	});

	it('providing the same signature multiple times to combining via different methods', async () => {
		const k1 = new Ed25519Keypair();
		const pk1 = k1.getPublicKey();

		const k2 = new Secp256k1Keypair();
		const pk2 = k2.getPublicKey();

		const multiSigPublicKey = MultiSigPublicKey.fromPublicKeys({
			threshold: 3,
			publicKeys: [
				{ publicKey: pk1, weight: 1 },
				{ publicKey: pk2, weight: 2 },
			],
		});

		const signData = new TextEncoder().encode('hello world');
		const sig1 = await k1.signPersonalMessage(signData);
		const sig2 = await k2.signPersonalMessage(signData);

		const isValidSig1 = await k1.getPublicKey().verifyPersonalMessage(signData, sig1.signature);
		const isValidSig2 = await k2.getPublicKey().verifyPersonalMessage(signData, sig2.signature);

		expect(isValidSig1).toBe(true);
		expect(isValidSig2).toBe(true);

		// create invalid signature
		const compressedSignatures: ({ ED25519: number[] } | { Secp256r1: number[] })[] = [
			{
				ED25519: Array.from(
					parseSerializedSignature(sig1.signature).signature!.map((x: number) => Number(x)),
				),
			},
			{
				ED25519: Array.from(
					parseSerializedSignature(sig1.signature).signature!.map((x: number) => Number(x)),
				),
			},
		];

		const bytes = bcs.MultiSig.serialize({
			sigs: compressedSignatures,
			bitmap: 1,
			multisig_pk: bcs.MultiSigPublicKey.parse(multiSigPublicKey.toRawBytes()),
		}).toBytes();
		let tmp = new Uint8Array(bytes.length + 1);
		tmp.set([SIGNATURE_SCHEME_TO_FLAG['MultiSig']]);
		tmp.set(bytes, 1);

		const multisig = toBase64(tmp);

		expect(() =>
			multiSigPublicKey.combinePartialSignatures([sig2.signature, sig2.signature]),
		).toThrowError(new Error('Received multiple signatures from the same public key'));

		const parsed = parseSerializedSignature(multisig);
		if (parsed.signatureScheme !== 'MultiSig') {
			throw new Error('Expected signature scheme to be MultiSig');
		}
		const publicKey = new MultiSigPublicKey(parsed.multisig!.multisig_pk);

		await expect(publicKey.verifyPersonalMessage(signData, multisig)).rejects.toThrow(
			new Error("Cannot read properties of undefined (reading 'pubKey')"),
		);
	});

	it('providing invalid signature', async () => {
		const k1 = new Ed25519Keypair();
		const pk1 = k1.getPublicKey();

		const k2 = new Secp256k1Keypair();
		const pk2 = k2.getPublicKey();

		const multiSigPublicKey = MultiSigPublicKey.fromPublicKeys({
			threshold: 3,
			publicKeys: [
				{ publicKey: pk1, weight: 1 },
				{ publicKey: pk2, weight: 2 },
			],
		});

		const signData = new TextEncoder().encode('hello world');
		const sig1 = await k1.signPersonalMessage(signData);
		const sig2 = await k2.signPersonalMessage(signData);

		// Invalid Signature.
		const sig3: SignatureWithBytes = {
			bytes: 'd',
			signature: 'd',
		};

		const isValidSig1 = await k1.getPublicKey().verifyPersonalMessage(signData, sig1.signature);
		const isValidSig2 = await k2.getPublicKey().verifyPersonalMessage(signData, sig2.signature);

		expect(isValidSig1).toBe(true);
		expect(isValidSig2).toBe(true);

		// publickey.ts
		expect(() => multiSigPublicKey.combinePartialSignatures([sig3.signature])).toThrowError();
	});

	it('providing signatures with invalid order', async () => {
		const k1 = new Secp256r1Keypair();
		const pk1 = k1.getPublicKey();

		const k2 = new Secp256k1Keypair();
		const pk2 = k2.getPublicKey();

		const multiSigPublicKey = MultiSigPublicKey.fromPublicKeys({
			threshold: 3,
			publicKeys: [
				{ publicKey: pk1, weight: 1 },
				{ publicKey: pk2, weight: 2 },
			],
		});

		const signData = new TextEncoder().encode('hello world');
		const sig1 = await k1.signPersonalMessage(signData);
		const sig2 = await k2.signPersonalMessage(signData);

		const isValidSig1 = await k1.getPublicKey().verifyPersonalMessage(signData, sig1.signature);
		const isValidSig2 = await k2.getPublicKey().verifyPersonalMessage(signData, sig2.signature);

		expect(isValidSig1).toBe(true);
		expect(isValidSig2).toBe(true);

		// publickey.ts
		const multisig = multiSigPublicKey.combinePartialSignatures([sig2.signature, sig1.signature]);

		const parsed = parseSerializedSignature(multisig);
		if (parsed.signatureScheme !== 'MultiSig') {
			throw new Error('Expected signature scheme to be MultiSig');
		}
		const publicKey = new MultiSigPublicKey(parsed.multisig!.multisig_pk);

		// Invalid order can't be verified.
		expect(await publicKey.verifyPersonalMessage(signData, multisig)).toEqual(false);
		expect(await multiSigPublicKey.verifyPersonalMessage(signData, multisig)).toEqual(false);
	});

	it('providing invalid intent scope', async () => {
		const k1 = new Secp256r1Keypair();
		const pk1 = k1.getPublicKey();

		const k2 = new Secp256k1Keypair();
		const pk2 = k2.getPublicKey();

		const multiSigPublicKey = MultiSigPublicKey.fromPublicKeys({
			threshold: 3,
			publicKeys: [
				{ publicKey: pk1, weight: 1 },
				{ publicKey: pk2, weight: 2 },
			],
		});

		const signData = new TextEncoder().encode('hello world');
		const sig1 = await k1.signPersonalMessage(signData);
		const sig2 = await k2.signPersonalMessage(signData);

		const isValidSig1 = await k1.getPublicKey().verifyPersonalMessage(signData, sig1.signature);
		const isValidSig2 = await k2.getPublicKey().verifyPersonalMessage(signData, sig2.signature);

		expect(isValidSig1).toBe(true);
		expect(isValidSig2).toBe(true);

		// publickey.ts
		const multisig = multiSigPublicKey.combinePartialSignatures([sig1.signature, sig2.signature]);

		const parsed = parseSerializedSignature(multisig);
		if (parsed.signatureScheme !== 'MultiSig') {
			throw new Error('Expected signature scheme to be MultiSig');
		}
		const publicKey = new MultiSigPublicKey(parsed.multisig!.multisig_pk);

		// Invalid intentScope.
		expect(await publicKey.verifyTransaction(signData, multisig)).toEqual(false);
		expect(await multiSigPublicKey.verifyTransaction(signData, multisig)).toEqual(false);
	});

	it('providing empty values', async () => {
		const k1 = new Secp256r1Keypair();
		const pk1 = k1.getPublicKey();

		const k2 = new Secp256k1Keypair();
		const pk2 = k2.getPublicKey();

		const multiSigPublicKey = MultiSigPublicKey.fromPublicKeys({
			threshold: 3,
			publicKeys: [
				{ publicKey: pk1, weight: 1 },
				{ publicKey: pk2, weight: 2 },
			],
		});

		const signData = new TextEncoder().encode('hello world');
		const sig1 = await k1.signPersonalMessage(signData);
		const sig2 = await k2.signPersonalMessage(signData);

		const isValidSig1 = await k1.getPublicKey().verifyPersonalMessage(signData, sig1.signature);
		const isValidSig2 = await k2.getPublicKey().verifyPersonalMessage(signData, sig2.signature);

		expect(isValidSig1).toBe(true);
		expect(isValidSig2).toBe(true);

		// Empty values.
		const multisig = multiSigPublicKey.combinePartialSignatures([]);

		const parsed = parseSerializedSignature(multisig);
		if (parsed.signatureScheme !== 'MultiSig') {
			throw new Error('Expected signature scheme to be MultiSig');
		}
		const publicKey = new MultiSigPublicKey(parsed.multisig!.multisig_pk);

		// Rejects verification.
		expect(await publicKey.verifyTransaction(signData, multisig)).toEqual(false);
		expect(await multiSigPublicKey.verifyTransaction(signData, multisig)).toEqual(false);
	});
});

describe('Multisig address creation:', () => {
	let k1: Ed25519Keypair,
		pk1: Ed25519PublicKey,
		k2: Secp256k1Keypair,
		pk2: PublicKey,
		k3: Secp256r1Keypair,
		pk3: PublicKey,
		pk4: PublicKey,
		pk5: PublicKey,
		k6: Ed25519Keypair,
		pk6: PublicKey;

	beforeAll(() => {
		k1 = new Ed25519Keypair();
		pk1 = k1.getPublicKey();

		k2 = new Secp256k1Keypair();
		pk2 = k2.getPublicKey();

		k3 = new Secp256r1Keypair();
		pk3 = k3.getPublicKey();

		pk4 = toZkLoginPublicIdentifier(
			BigInt('20794788559620669596206457022966176986688727876128223628113916380927502737911'),
			'https://id.twitch.tv/oauth2',
		);
		pk5 = toZkLoginPublicIdentifier(
			BigInt('380704556853533152350240698167704405529973457670972223618755249929828551006'),
			'https://id.twitch.tv/oauth2',
		);

		const secret_key_ed25519 = new Uint8Array([
			126, 57, 195, 235, 248, 196, 105, 68, 115, 164, 8, 221, 100, 250, 137, 160, 245, 43, 220, 168,
			250, 73, 119, 95, 19, 242, 100, 105, 81, 114, 86, 105,
		]);
		k6 = Ed25519Keypair.fromSecretKey(secret_key_ed25519);
		pk6 = k6.getPublicKey();
	});

	it('`toMultiSigAddress()` with zklogin identifiers', async () => {
		// Test derived from rust test `fn test_derive_multisig_address()`
		const multisigPublicKey = MultiSigPublicKey.fromPublicKeys({
			publicKeys: [
				{ publicKey: pk4, weight: 1 },
				{ publicKey: pk5, weight: 1 },
			],
			threshold: 1,
		});
		const multisigAddress = multisigPublicKey.toSuiAddress();

		expect(multisigAddress).toEqual(
			'0x77a9fbf3c695d78dd83449a81a9e70aa79a77dbfd6fb72037bf09201c12052cd',
		);
	});

	it('`combinePartialSigs()` with zklogin sigs', async () => {
		// Test derived from rust test `fn multisig_zklogin_scenarios()`
		const publicKey = MultiSigPublicKey.fromPublicKeys({
			publicKeys: [
				{ publicKey: pk6, weight: 1 },
				{ publicKey: pk4, weight: 1 },
			],
			threshold: 1,
		});
		expect(publicKey.toSuiAddress()).toEqual(
			'0xb9c0780a3943cde13a2409bf1a6f06ae60b0dff2b2f373260cf627aa4f43a588',
		);
		const data = new Uint8Array(
			fromBase64(
				'AAABACACAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgEBAQABAAC5wHgKOUPN4TokCb8abwauYLDf8rLzcyYM9ieqT0OliAGbB4FfBEl+LgXSLKw6oGFBCyCGjMYZFUxCocYb6ZAnFwEAAAAAAAAAIJZw7UpW1XHubORIOaY8d2+WyBNwoJ+FEAxlsa7h7JHrucB4CjlDzeE6JAm/Gm8GrmCw3/Ky83MmDPYnqk9DpYgBAAAAAAAAABAnAAAAAAAAAA==',
			),
		);
		const sig1 = await k6.signTransaction(data);
		const zklogin_sig =
			'BQNNMTczMTgwODkxMjU5NTI0MjE3MzYzNDIyNjM3MTc5MzI3MTk0Mzc3MTc4NDQyODI0MTAxODc5NTc5ODQ3NTE5Mzk5NDI4OTgyNTEyNTBNMTEzNzM5NjY2NDU0NjkxMjI1ODIwNzQwODIyOTU5ODUzODgyNTg4NDA2ODE2MTgyNjg1OTM5NzY2OTczMjU4OTIyODA5MTU2ODEyMDcBMQMCTDU5Mzk4NzExNDczNDg4MzQ5OTczNjE3MjAxMjIyMzg5ODAxNzcxNTIzMDMyNzQzMTEwNDcyNDk5MDU5NDIzODQ5MTU3Njg2OTA4OTVMNDUzMzU2ODI3MTEzNDc4NTI3ODczMTIzNDU3MDM2MTQ4MjY1MTk5Njc0MDc5MTg4ODI4NTg2NDk2Njg4NDAzMjcxNzA0OTgxMTcwOAJNMTA1NjQzODcyODUwNzE1NTU0Njk3NTM5OTA2NjE0MTA4NDAxMTg2MzU5MjU0NjY1OTcwMzcwMTgwNTg3NzAwNDEzNDc1MTg0NjEzNjhNMTI1OTczMjM1NDcyNzc1NzkxNDQ2OTg0OTYzNzIyNDI2MTUzNjgwODU4MDEzMTMzNDMxNTU3MzU1MTEzMzAwMDM4ODQ3Njc5NTc4NTQCATEBMANNMTU3OTE1ODk0NzI1NTY4MjYyNjMyMzE2NDQ3Mjg4NzMzMzc2MjkwMTUyNjk5ODQ2OTk0MDQwNzM2MjM2MDMzNTI1Mzc2Nzg4MTMxNzFMNDU0Nzg2NjQ5OTI0ODg4MTQ0OTY3NjE2MTE1ODAyNDc0ODA2MDQ4NTM3MzI1MDAyOTQyMzkwNDExMzAxNzQyMjUzOTAzNzE2MjUyNwExMXdpYVhOeklqb2lhSFIwY0hNNkx5OXBaQzUwZDJsMFkyZ3VkSFl2YjJGMWRHZ3lJaXcCMmV5SmhiR2NpT2lKU1V6STFOaUlzSW5SNWNDSTZJa3BYVkNJc0ltdHBaQ0k2SWpFaWZRTTIwNzk0Nzg4NTU5NjIwNjY5NTk2MjA2NDU3MDIyOTY2MTc2OTg2Njg4NzI3ODc2MTI4MjIzNjI4MTEzOTE2MzgwOTI3NTAyNzM3OTExCgAAAAAAAABhABHpkQ5JvxqbqCKtqh9M0U5c3o3l62B6ALVOxMq6nsc0y3JlY8Gf1ZoPA976dom6y3JGBUTsry6axfqHcVrtRAy5xu4WMO8+cRFEpkjbBruyKE9ydM++5T/87lA8waSSAA==';
		const parsed_zklogin_sig = parseSerializedSignature(zklogin_sig);
		const multisig = publicKey.combinePartialSignatures([sig1.signature, zklogin_sig]);
		expect(multisig).toEqual(
			'AwIAcAEsWrZtlsE3AdGUKJAPag8Tu6HPfMW7gEemeneO9fmNGiJP/rDZu/tL75lr8A22eFDx9K2G1DL4v8XlmuTtCgOaBwUDTTE3MzE4MDg5MTI1OTUyNDIxNzM2MzQyMjYzNzE3OTMyNzE5NDM3NzE3ODQ0MjgyNDEwMTg3OTU3OTg0NzUxOTM5OTQyODk4MjUxMjUwTTExMzczOTY2NjQ1NDY5MTIyNTgyMDc0MDgyMjk1OTg1Mzg4MjU4ODQwNjgxNjE4MjY4NTkzOTc2Njk3MzI1ODkyMjgwOTE1NjgxMjA3ATEDAkw1OTM5ODcxMTQ3MzQ4ODM0OTk3MzYxNzIwMTIyMjM4OTgwMTc3MTUyMzAzMjc0MzExMDQ3MjQ5OTA1OTQyMzg0OTE1NzY4NjkwODk1TDQ1MzM1NjgyNzExMzQ3ODUyNzg3MzEyMzQ1NzAzNjE0ODI2NTE5OTY3NDA3OTE4ODgyODU4NjQ5NjY4ODQwMzI3MTcwNDk4MTE3MDgCTTEwNTY0Mzg3Mjg1MDcxNTU1NDY5NzUzOTkwNjYxNDEwODQwMTE4NjM1OTI1NDY2NTk3MDM3MDE4MDU4NzcwMDQxMzQ3NTE4NDYxMzY4TTEyNTk3MzIzNTQ3Mjc3NTc5MTQ0Njk4NDk2MzcyMjQyNjE1MzY4MDg1ODAxMzEzMzQzMTU1NzM1NTExMzMwMDAzODg0NzY3OTU3ODU0AgExATADTTE1NzkxNTg5NDcyNTU2ODI2MjYzMjMxNjQ0NzI4ODczMzM3NjI5MDE1MjY5OTg0Njk5NDA0MDczNjIzNjAzMzUyNTM3Njc4ODEzMTcxTDQ1NDc4NjY0OTkyNDg4ODE0NDk2NzYxNjExNTgwMjQ3NDgwNjA0ODUzNzMyNTAwMjk0MjM5MDQxMTMwMTc0MjI1MzkwMzcxNjI1MjcBMTF3aWFYTnpJam9pYUhSMGNITTZMeTlwWkM1MGQybDBZMmd1ZEhZdmIyRjFkR2d5SWl3AjJleUpoYkdjaU9pSlNVekkxTmlJc0luUjVjQ0k2SWtwWFZDSXNJbXRwWkNJNklqRWlmUU0yMDc5NDc4ODU1OTYyMDY2OTU5NjIwNjQ1NzAyMjk2NjE3Njk4NjY4ODcyNzg3NjEyODIyMzYyODExMzkxNjM4MDkyNzUwMjczNzkxMQoAAAAAAAAAYQAR6ZEOSb8am6giraofTNFOXN6N5etgegC1TsTKup7HNMtyZWPBn9WaDwPe+naJustyRgVE7K8umsX6h3Fa7UQMucbuFjDvPnERRKZI2wa7sihPcnTPvuU//O5QPMGkkgADAAIADX2rNYyNrapO+gBJp1sHQ2VVsQo2ghm7aA9wVxNJ13UBAzwbaHR0cHM6Ly9pZC50d2l0Y2gudHYvb2F1dGgyLflu6Eag/zG3tLd5CtZRYx9p1t34RovVSn/+uHFiYfcBAQA=',
		);

		const decoded = parsePartialSignatures(bcs.MultiSig.parse(fromBase64(multisig).slice(1)));
		expect(decoded).toEqual([
			{
				signature: parseSerializedSignature(sig1.signature).signature,
				signatureScheme: k6.getKeyScheme(),
				publicKey: pk6,
				weight: 1,
			},
			{
				signature: parsed_zklogin_sig.signature,
				signatureScheme: 'ZkLogin',
				publicKey: pk4,
				weight: 1,
			},
		]);
	});

	it('with unreachable threshold', async () => {
		expect(() =>
			MultiSigPublicKey.fromPublicKeys({
				threshold: 7,
				publicKeys: [
					{ publicKey: pk1, weight: 1 },
					{ publicKey: pk2, weight: 2 },
					{ publicKey: pk3, weight: 3 },
				],
			}),
		).toThrow(new Error('Unreachable threshold'));
	});

	it('with more public keys than limited number', async () => {
		const k4 = new Secp256r1Keypair();
		const pk4 = k4.getPublicKey();

		const k5 = new Secp256r1Keypair();
		const pk5 = k5.getPublicKey();

		const k6 = new Secp256r1Keypair();
		const pk6 = k6.getPublicKey();

		const k7 = new Secp256r1Keypair();
		const pk7 = k7.getPublicKey();

		const k8 = new Secp256r1Keypair();
		const pk8 = k8.getPublicKey();

		const k9 = new Secp256r1Keypair();
		const pk9 = k9.getPublicKey();

		const k10 = new Secp256r1Keypair();
		const pk10 = k10.getPublicKey();

		const k11 = new Secp256r1Keypair();
		const pk11 = k11.getPublicKey();

		expect(() =>
			MultiSigPublicKey.fromPublicKeys({
				threshold: 10,
				publicKeys: [
					{ publicKey: pk1, weight: 1 },
					{ publicKey: pk2, weight: 2 },
					{ publicKey: pk3, weight: 3 },
					{ publicKey: pk4, weight: 4 },
					{ publicKey: pk5, weight: 5 },
					{ publicKey: pk6, weight: 1 },
					{ publicKey: pk7, weight: 2 },
					{ publicKey: pk8, weight: 3 },
					{ publicKey: pk9, weight: 4 },
					{ publicKey: pk10, weight: 5 },
					{ publicKey: pk11, weight: 6 },
				],
			}),
		).toThrowError(new Error('Max number of signers in a multisig is 10'));
	});

	it('with max weights and max threshold values', async () => {
		expect(() =>
			MultiSigPublicKey.fromPublicKeys({
				threshold: 65535,
				publicKeys: [
					{ publicKey: pk1, weight: 1 },
					{ publicKey: pk2, weight: 256 },
					{ publicKey: pk3, weight: 3 },
				],
			}),
		).toThrow(new Error('Invalid u8 value: 256. Expected value in range 0-255'));

		expect(() =>
			MultiSigPublicKey.fromPublicKeys({
				threshold: 65536,
				publicKeys: [
					{ publicKey: pk1, weight: 1 },
					{ publicKey: pk2, weight: 2 },
					{ publicKey: pk3, weight: 3 },
				],
			}),
		).toThrow(new Error('Invalid u16 value: 65536. Expected value in range 0-65535'));
	});

	it('with zero weight value', async () => {
		expect(() =>
			MultiSigPublicKey.fromPublicKeys({
				threshold: 10,
				publicKeys: [
					{ publicKey: pk1, weight: 0 },
					{ publicKey: pk2, weight: 6 },
					{ publicKey: pk3, weight: 10 },
				],
			}),
		).toThrow(new Error('Invalid weight'));
	});

	it('with zero threshold value', async () => {
		expect(() =>
			MultiSigPublicKey.fromPublicKeys({
				threshold: 0,
				publicKeys: [
					{ publicKey: pk1, weight: 1 },
					{ publicKey: pk2, weight: 2 },
					{ publicKey: pk3, weight: 3 },
				],
			}),
		).toThrow(new Error('Invalid threshold'));
	});

	it('with empty values', async () => {
		expect(() =>
			MultiSigPublicKey.fromPublicKeys({
				threshold: 2,
				publicKeys: [],
			}),
		).toThrow(new Error('Unreachable threshold'));
	});

	it('with duplicated publickeys', async () => {
		expect(() =>
			MultiSigPublicKey.fromPublicKeys({
				threshold: 4,
				publicKeys: [
					{ publicKey: pk1, weight: 1 },
					{ publicKey: pk1, weight: 2 },
					{ publicKey: pk3, weight: 3 },
				],
			}),
		).toThrow(new Error('Multisig does not support duplicate public keys'));
	});
});

describe('MultisigKeypair', () => {
	test('signTransaction', async () => {
		const k1 = new Ed25519Keypair();
		const pk1 = k1.getPublicKey();

		const k2 = new Secp256k1Keypair();
		const pk2 = k2.getPublicKey();

		const k3 = new Secp256r1Keypair();
		const pk3 = k3.getPublicKey();

		const pubkeyWeightPairs = [
			{
				publicKey: pk1,
				weight: 1,
			},
			{
				publicKey: pk2,
				weight: 2,
			},
			{
				publicKey: pk3,
				weight: 3,
			},
		];

		const tx = new Transaction();
		tx.setSender(k3.getPublicKey().toSuiAddress());
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

		const publicKey = MultiSigPublicKey.fromPublicKeys({
			threshold: 3,
			publicKeys: pubkeyWeightPairs,
		});

		const signer = publicKey.getSigner(k3);
		const signer2 = new MultiSigSigner(publicKey, [k1, k2]);

		const multisig = await signer.signTransaction(bytes);
		const multisig2 = await signer2.signTransaction(bytes);

		const parsed = parseSerializedSignature(multisig.signature);
		if (parsed.signatureScheme !== 'MultiSig') {
			throw new Error('Expected signature scheme to be MultiSig');
		}

		const signerPubKey = await verifyTransactionSignature(bytes, multisig.signature);
		expect(signerPubKey.toSuiAddress()).toEqual(publicKey.toSuiAddress());
		expect(await publicKey.verifyTransaction(bytes, multisig.signature)).toEqual(true);
		const signerPubKey2 = await verifyTransactionSignature(bytes, multisig2.signature);
		expect(signerPubKey2.toSuiAddress()).toEqual(publicKey.toSuiAddress());
		expect(await publicKey.verifyTransaction(bytes, multisig2.signature)).toEqual(true);
	});

	test('signPersonalMessage', async () => {
		const k1 = new Ed25519Keypair();
		const pk1 = k1.getPublicKey();

		const k2 = new Secp256k1Keypair();
		const pk2 = k2.getPublicKey();

		const k3 = new Secp256r1Keypair();
		const pk3 = k3.getPublicKey();

		const pubkeyWeightPairs = [
			{
				publicKey: pk1,
				weight: 1,
			},
			{
				publicKey: pk2,
				weight: 2,
			},
			{
				publicKey: pk3,
				weight: 3,
			},
		];

		const bytes = new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9]);

		const publicKey = MultiSigPublicKey.fromPublicKeys({
			threshold: 3,
			publicKeys: pubkeyWeightPairs,
		});

		const signer = publicKey.getSigner(k3);
		const signer2 = new MultiSigSigner(publicKey, [k1, k2]);

		const multisig = await signer.signPersonalMessage(bytes);
		const multisig2 = await signer2.signPersonalMessage(bytes);

		const parsed = parseSerializedSignature(multisig.signature);
		if (parsed.signatureScheme !== 'MultiSig') {
			throw new Error('Expected signature scheme to be MultiSig');
		}

		const signerPubKey = await verifyPersonalMessageSignature(bytes, multisig.signature);
		expect(signerPubKey.toSuiAddress()).toEqual(publicKey.toSuiAddress());
		expect(await publicKey.verifyPersonalMessage(bytes, multisig.signature)).toEqual(true);
		const signerPubKey2 = await verifyPersonalMessageSignature(bytes, multisig2.signature);
		expect(signerPubKey2.toSuiAddress()).toEqual(publicKey.toSuiAddress());
		expect(await publicKey.verifyPersonalMessage(bytes, multisig2.signature)).toEqual(true);
	});

	test('duplicate signers', async () => {
		const k1 = new Ed25519Keypair();
		const pk1 = k1.getPublicKey();

		const k2 = new Secp256k1Keypair();
		const pk2 = k2.getPublicKey();

		const k3 = new Secp256r1Keypair();
		const pk3 = k3.getPublicKey();

		const pubkeyWeightPairs = [
			{
				publicKey: pk1,
				weight: 1,
			},
			{
				publicKey: pk2,
				weight: 2,
			},
			{
				publicKey: pk3,
				weight: 3,
			},
		];

		const publicKey = MultiSigPublicKey.fromPublicKeys({
			threshold: 3,
			publicKeys: pubkeyWeightPairs,
		});

		expect(() => new MultiSigSigner(publicKey, [k1, k1])).toThrow(
			new Error(`Can't create MultiSigSigner with duplicate signers`),
		);
	});

	test('insufficient weight', async () => {
		const k1 = new Ed25519Keypair();
		const pk1 = k1.getPublicKey();

		const k2 = new Secp256k1Keypair();
		const pk2 = k2.getPublicKey();

		const k3 = new Secp256r1Keypair();
		const pk3 = k3.getPublicKey();

		const pubkeyWeightPairs = [
			{
				publicKey: pk1,
				weight: 1,
			},
			{
				publicKey: pk2,
				weight: 2,
			},
			{
				publicKey: pk3,
				weight: 3,
			},
		];

		const publicKey = MultiSigPublicKey.fromPublicKeys({
			threshold: 3,
			publicKeys: pubkeyWeightPairs,
		});

		expect(() => publicKey.getSigner(k1)).toThrow(
			new Error(`Combined weight of signers is less than threshold`),
		);
	});

	test('unknown signers', async () => {
		const k1 = new Ed25519Keypair();
		const pk1 = k1.getPublicKey();

		const k2 = new Secp256k1Keypair();
		const pk2 = k2.getPublicKey();

		const pubkeyWeightPairs = [
			{
				publicKey: pk1,
				weight: 1,
			},
		];

		const publicKey = MultiSigPublicKey.fromPublicKeys({
			threshold: 1,
			publicKeys: pubkeyWeightPairs,
		});

		expect(() => publicKey.getSigner(k2)).toThrow(
			new Error(`Signer ${pk2.toSuiAddress()} is not part of the MultiSig public key`),
		);
	});
});
