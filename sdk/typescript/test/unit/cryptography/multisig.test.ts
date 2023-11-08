// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { beforeAll, describe, expect, it } from 'vitest';

import { parseSerializedSignature } from '../../../src/cryptography';
import {
	combinePartialSigs,
	decodeMultiSig,
	MAX_SIGNER_IN_MULTISIG,
	PubkeyWeightPair,
	toMultiSigAddress,
} from '../../../src/cryptography/multisig';
import { PublicKey } from '../../../src/cryptography/publickey';
import { Ed25519Keypair, Ed25519PublicKey } from '../../../src/keypairs/ed25519';
import { Secp256k1Keypair } from '../../../src/keypairs/secp256k1';
import { Secp256r1Keypair } from '../../../src/keypairs/secp256r1';
import { toZkLoginPublicIdentifier } from '../../../src/keypairs/zklogin/publickey';
import { MultiSigPublicKey } from '../../../src/multisig/publickey';
import { fromB64 } from '@mysten/bcs';


describe('multisig address and combine sigs', () => {
	// Address and combined multisig matches rust impl: fn multisig_serde_test()
	it('combines signature to multisig', async () => {
		const VALID_SECP256K1_SECRET_KEY = [
			59, 148, 11, 85, 134, 130, 61, 253, 2, 174, 59, 70, 27, 180, 51, 107, 94, 203, 174, 253, 102,
			39, 170, 146, 46, 252, 4, 143, 236, 12, 136, 28,
		];
		const secret_key = new Uint8Array(VALID_SECP256K1_SECRET_KEY);
		let k1 = Ed25519Keypair.fromSecretKey(secret_key);
		let pk1 = k1.getPublicKey();

		let k2 = Secp256k1Keypair.fromSecretKey(secret_key);
		let pk2 = k2.getPublicKey();

		let k3 = Ed25519Keypair.fromSecretKey(new Uint8Array(32).fill(0));
		let pk3 = k3.getPublicKey();

		const multiSigPublicKey = MultiSigPublicKey.fromPublicKeys({
			threshold: 3,
			publicKeys: [
				{ publicKey: pk1, weight: 1 },
				{ publicKey: pk2, weight: 2 },
				{ publicKey: pk3, weight: 3 },
			],
		});

		const data = new Uint8Array([0, 0, 0, 5, 72, 101, 108, 108, 111]);

		const sig1 = await k1.signPersonalMessage(data);
		const sig2 = await k2.signPersonalMessage(data);
		const sig3 = await k3.signPersonalMessage(data);

		expect(multiSigPublicKey.toSuiAddress()).toEqual(
			'0x37b048598ca569756146f4e8ea41666c657406db154a31f11bb5c1cbaf0b98d7',
		);

		let multisig = multiSigPublicKey.combinePartialSignatures([sig1.signature, sig2.signature]);
		expect(multisig).toEqual(
			'AwIANe9gJJmT5m1UvpV8Hj7nOyif76rS5Zgg1bi7VApts+KwtSc2Bg8WJ6LBfGnZKugrOqtQsk5d2Q+IMRLD4hYmBQFYlrlXc01/ZSdgwSD3eGEdm6kxwtOwAvTWdb2wNZP2Hnkgrh+indYN4s2Qd99iYCz+xsY6aT5lpOBsDZb2x9LyAwADAFriILSy9l6XfBLt5hV5/1FwtsIsAGFow3tefGGvAYCDAQECHRUjB8a3Kw7QQYsOcM2A5/UpW42G9XItP1IT+9I5TzYCADtqJ7zOtqQtYqOo0CpvDXNlMhV3HeJDpjrASKGLWdopAwMA',
		);

		let decoded = decodeMultiSig(multisig);
		expect(decoded).toEqual([
			{
				signature: parseSerializedSignature((await k1.signPersonalMessage(data)).signature)
					.signature,
				signatureScheme: k1.getKeyScheme(),
				pubKey: pk1,
				weight: 1,
			},
			{
				signature: parseSerializedSignature((await k2.signPersonalMessage(data)).signature)
					.signature,
				signatureScheme: k2.getKeyScheme(),
				pubKey: pk2,
				weight: 2,
			},
		]);

		const parsed = parseSerializedSignature(multisig);
		const publicKey = new MultiSigPublicKey(parsed.multisig!.multisig_pk);
		// multisig (sig1 + sig2 weight 1+2 >= threshold ) verifies ok
		expect(await publicKey.verifyPersonalMessage(data, multisig)).toEqual(true);

		let multisig2 = parseSerializedSignature(
			multiSigPublicKey.combinePartialSignatures([sig3.signature]),
		);

		// multisig (sig3 only weight = 3 >= threshold) verifies ok
		expect(
			await multiSigPublicKey.verifyPersonalMessage(data, multisig2.serializedSignature),
		).toEqual(true);

		let multisig3 = parseSerializedSignature(
			multiSigPublicKey.combinePartialSignatures([sig2.signature]),
		);

		// multisig (sig2 only weight = 2 < threshold) verify fails

		expect(
			await new MultiSigPublicKey(multisig3.multisig!.multisig_pk).verifyPersonalMessage(
				data,
				multisig3.serializedSignature,
			),
		).toEqual(false);
	});
});

describe('Multisig', () => {
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
		const VALID_SECP256K1_SECRET_KEY = [
			59, 148, 11, 85, 134, 130, 61, 253, 2, 174, 59, 70, 27, 180, 51, 107, 94, 203, 174, 253, 102,
			39, 170, 146, 46, 252, 4, 143, 236, 12, 136, 28,
		];

		const VALID_SECP256R1_SECRET_KEY = [
			66, 37, 141, 205, 161, 76, 241, 17, 198, 2, 184, 151, 27, 140, 200, 67, 233, 30, 70, 202, 144,
			81, 81, 192, 39, 68, 166, 176, 23, 230, 147, 22,
		];

		const secret_key_k1 = new Uint8Array(VALID_SECP256K1_SECRET_KEY);
		const secret_key_r1 = new Uint8Array(VALID_SECP256R1_SECRET_KEY);
		const secret_key_ed25519 = new Uint8Array([126, 57, 195, 235, 248, 196, 105, 68, 115, 164, 8, 221, 100, 250, 137, 160, 245, 43, 220, 168, 250, 73, 119, 95, 19, 242, 100, 105, 81, 114, 86, 105]);

		k1 = Ed25519Keypair.fromSecretKey(secret_key_k1);
		pk1 = k1.getPublicKey();

		k2 = Secp256k1Keypair.fromSecretKey(secret_key_k1);
		pk2 = k2.getPublicKey();

		k3 = Secp256r1Keypair.fromSecretKey(secret_key_r1);
		pk3 = k3.getPublicKey();
		
		pk4 = toZkLoginPublicIdentifier(
			BigInt('20794788559620669596206457022966176986688727876128223628113916380927502737911'),
			'https://id.twitch.tv/oauth2',
		);
		pk5 = toZkLoginPublicIdentifier(
			BigInt('380704556853533152350240698167704405529973457670972223618755249929828551006'),
			'https://id.twitch.tv/oauth2',
		);

		k6 = Ed25519Keypair.fromSecretKey(secret_key_ed25519);
		pk6 = k6.getPublicKey();
	});

	it('`toMultiSigAddress()` should derive a multisig address correctly', async () => {
		const pubkeyWeightPairs: PubkeyWeightPair[] = [
			{
				pubKey: pk1,
				weight: 1,
			},
			{
				pubKey: pk2,
				weight: 2,
			},
			{
				pubKey: pk3,
				weight: 3,
			},
		];

		const multisigAddress = toMultiSigAddress(pubkeyWeightPairs, 3);

		expect(multisigAddress).toEqual(
			'0x8ee027fe556a3f6c0a23df64f090d2429fec0bb21f55594783476e81de2dec27',
		);
	});

	it('`toMultiSigAddress()` with zklogin identifiers', async () => {
		const pubkeyWeightPairs: PubkeyWeightPair[] = [
			{
				pubKey: pk4,
				weight: 1,
			},
			{
				pubKey: pk5,
				weight: 1,
			},
		];

		const multisigAddress = toMultiSigAddress(pubkeyWeightPairs, 1);

		expect(multisigAddress).toEqual(
			'0x77a9fbf3c695d78dd83449a81a9e70aa79a77dbfd6fb72037bf09201c12052cd',
		);
	});

	it('`toMultiSigAddress()` should throw an error when exceeding the max number of signers', async () => {
		const pubkeyWeightPairs: PubkeyWeightPair[] = new Array(MAX_SIGNER_IN_MULTISIG + 1).fill({
			pubKey: pk1,
			weight: 1,
		});

		expect(() => toMultiSigAddress(pubkeyWeightPairs, 3)).toThrowError(
			new Error(`Max number of signers in a multisig is ${MAX_SIGNER_IN_MULTISIG}`),
		);
	});

	it('`combinePartialSigs()` should combine with different signatures into a single multisig correctly', async () => {
		const pubkeyWeightPairs: PubkeyWeightPair[] = [
			{
				pubKey: pk1,
				weight: 1,
			},
			{
				pubKey: pk2,
				weight: 2,
			},
			{
				pubKey: pk3,
				weight: 3,
			},
		];

		const data = new Uint8Array([0, 0, 0, 5, 72, 101, 108, 108, 111]);

		const sig1 = await k1.signPersonalMessage(data);
		const sig2 = await k2.signPersonalMessage(data);

		const multisig = combinePartialSigs([sig1.signature, sig2.signature], pubkeyWeightPairs, 3);

		expect(multisig).toEqual(
			'AwIANe9gJJmT5m1UvpV8Hj7nOyif76rS5Zgg1bi7VApts+KwtSc2Bg8WJ6LBfGnZKugrOqtQsk5d2Q+IMRLD4hYmBQFYlrlXc01/ZSdgwSD3eGEdm6kxwtOwAvTWdb2wNZP2Hnkgrh+indYN4s2Qd99iYCz+xsY6aT5lpOBsDZb2x9LyAwADAFriILSy9l6XfBLt5hV5/1FwtsIsAGFow3tefGGvAYCDAQECHRUjB8a3Kw7QQYsOcM2A5/UpW42G9XItP1IT+9I5TzYCAgInMis6iRoKKA1rwfssuyPSj1SQb9ZAf190H23vV2JgmgMDAA==',
		);

		const decoded = decodeMultiSig(multisig);
		expect(decoded).toEqual([
			{
				signature: parseSerializedSignature((await k1.signPersonalMessage(data)).signature)
					.signature,
				signatureScheme: k1.getKeyScheme(),
				pubKey: pk1,
				weight: 1,
			},
			{
				signature: parseSerializedSignature((await k2.signPersonalMessage(data)).signature)
					.signature,
				signatureScheme: k2.getKeyScheme(),
				pubKey: pk2,
				weight: 2,
			},
		]);
	});

	it('`combinePartialSigs()` with zklogin sigs', async () => {
		const pubkeyWeightPairs: PubkeyWeightPair[] = [
			{
				pubKey: pk6, // traditional ed25519 key
				weight: 1,
			},
			{
				pubKey: pk4, // zk public identifier
				weight: 1,
			},
		];
		expect(toMultiSigAddress(pubkeyWeightPairs, 1)).toEqual(
			'0xb9c0780a3943cde13a2409bf1a6f06ae60b0dff2b2f373260cf627aa4f43a588',
		);
		const data = new Uint8Array(fromB64("AAABACACAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgEBAQABAAC5wHgKOUPN4TokCb8abwauYLDf8rLzcyYM9ieqT0OliAGbB4FfBEl+LgXSLKw6oGFBCyCGjMYZFUxCocYb6ZAnFwEAAAAAAAAAIJZw7UpW1XHubORIOaY8d2+WyBNwoJ+FEAxlsa7h7JHrucB4CjlDzeE6JAm/Gm8GrmCw3/Ky83MmDPYnqk9DpYgBAAAAAAAAABAnAAAAAAAAAA=="));
		console.log('pk6', pk6);
		const sig1 = await k6.signTransactionBlock(data);
		console.log('qqqqqq', sig1.signature);
		const zklogin_sig =
			'BQNNMTczMTgwODkxMjU5NTI0MjE3MzYzNDIyNjM3MTc5MzI3MTk0Mzc3MTc4NDQyODI0MTAxODc5NTc5ODQ3NTE5Mzk5NDI4OTgyNTEyNTBNMTEzNzM5NjY2NDU0NjkxMjI1ODIwNzQwODIyOTU5ODUzODgyNTg4NDA2ODE2MTgyNjg1OTM5NzY2OTczMjU4OTIyODA5MTU2ODEyMDcBMQMCTDU5Mzk4NzExNDczNDg4MzQ5OTczNjE3MjAxMjIyMzg5ODAxNzcxNTIzMDMyNzQzMTEwNDcyNDk5MDU5NDIzODQ5MTU3Njg2OTA4OTVMNDUzMzU2ODI3MTEzNDc4NTI3ODczMTIzNDU3MDM2MTQ4MjY1MTk5Njc0MDc5MTg4ODI4NTg2NDk2Njg4NDAzMjcxNzA0OTgxMTcwOAJNMTA1NjQzODcyODUwNzE1NTU0Njk3NTM5OTA2NjE0MTA4NDAxMTg2MzU5MjU0NjY1OTcwMzcwMTgwNTg3NzAwNDEzNDc1MTg0NjEzNjhNMTI1OTczMjM1NDcyNzc1NzkxNDQ2OTg0OTYzNzIyNDI2MTUzNjgwODU4MDEzMTMzNDMxNTU3MzU1MTEzMzAwMDM4ODQ3Njc5NTc4NTQCATEBMANNMTU3OTE1ODk0NzI1NTY4MjYyNjMyMzE2NDQ3Mjg4NzMzMzc2MjkwMTUyNjk5ODQ2OTk0MDQwNzM2MjM2MDMzNTI1Mzc2Nzg4MTMxNzFMNDU0Nzg2NjQ5OTI0ODg4MTQ0OTY3NjE2MTE1ODAyNDc0ODA2MDQ4NTM3MzI1MDAyOTQyMzkwNDExMzAxNzQyMjUzOTAzNzE2MjUyNwExMXdpYVhOeklqb2lhSFIwY0hNNkx5OXBaQzUwZDJsMFkyZ3VkSFl2YjJGMWRHZ3lJaXcCMmV5SmhiR2NpT2lKU1V6STFOaUlzSW5SNWNDSTZJa3BYVkNJc0ltdHBaQ0k2SWpFaWZRTTIwNzk0Nzg4NTU5NjIwNjY5NTk2MjA2NDU3MDIyOTY2MTc2OTg2Njg4NzI3ODc2MTI4MjIzNjI4MTEzOTE2MzgwOTI3NTAyNzM3OTExCgAAAAAAAABhABHpkQ5JvxqbqCKtqh9M0U5c3o3l62B6ALVOxMq6nsc0y3JlY8Gf1ZoPA976dom6y3JGBUTsry6axfqHcVrtRAy5xu4WMO8+cRFEpkjbBruyKE9ydM++5T/87lA8waSSAA==';
		// let sliced = parseZkLoginSignature(fromB64(zklogin_sig).slice(1));
		// console.log('sliced', sliced);
		const multisig = combinePartialSigs([sig1.signature, zklogin_sig], pubkeyWeightPairs, 1);
		console.log('multisig======', multisig);
		expect(multisig).toEqual(
			'AwIAcAEsWrZtlsE3AdGUKJAPag8Tu6HPfMW7gEemeneO9fmNGiJP/rDZu/tL75lr8A22eFDx9K2G1DL4v8XlmuTtCgMDTTE3MzE4MDg5MTI1OTUyNDIxNzM2MzQyMjYzNzE3OTMyNzE5NDM3NzE3ODQ0MjgyNDEwMTg3OTU3OTg0NzUxOTM5OTQyODk4MjUxMjUwTTExMzczOTY2NjQ1NDY5MTIyNTgyMDc0MDgyMjk1OTg1Mzg4MjU4ODQwNjgxNjE4MjY4NTkzOTc2Njk3MzI1ODkyMjgwOTE1NjgxMjA3ATEDAkw1OTM5ODcxMTQ3MzQ4ODM0OTk3MzYxNzIwMTIyMjM4OTgwMTc3MTUyMzAzMjc0MzExMDQ3MjQ5OTA1OTQyMzg0OTE1NzY4NjkwODk1TDQ1MzM1NjgyNzExMzQ3ODUyNzg3MzEyMzQ1NzAzNjE0ODI2NTE5OTY3NDA3OTE4ODgyODU4NjQ5NjY4ODQwMzI3MTcwNDk4MTE3MDgCTTEwNTY0Mzg3Mjg1MDcxNTU1NDY5NzUzOTkwNjYxNDEwODQwMTE4NjM1OTI1NDY2NTk3MDM3MDE4MDU4NzcwMDQxMzQ3NTE4NDYxMzY4TTEyNTk3MzIzNTQ3Mjc3NTc5MTQ0Njk4NDk2MzcyMjQyNjE1MzY4MDg1ODAxMzEzMzQzMTU1NzM1NTExMzMwMDAzODg0NzY3OTU3ODU0AgExATADTTE1NzkxNTg5NDcyNTU2ODI2MjYzMjMxNjQ0NzI4ODczMzM3NjI5MDE1MjY5OTg0Njk5NDA0MDczNjIzNjAzMzUyNTM3Njc4ODEzMTcxTDQ1NDc4NjY0OTkyNDg4ODE0NDk2NzYxNjExNTgwMjQ3NDgwNjA0ODUzNzMyNTAwMjk0MjM5MDQxMTMwMTc0MjI1MzkwMzcxNjI1MjcBMTF3aWFYTnpJam9pYUhSMGNITTZMeTlwWkM1MGQybDBZMmd1ZEhZdmIyRjFkR2d5SWl3AjJleUpoYkdjaU9pSlNVekkxTmlJc0luUjVjQ0k2SWtwWFZDSXNJbXRwWkNJNklqRWlmUU0yMDc5NDc4ODU1OTYyMDY2OTU5NjIwNjQ1NzAyMjk2NjE3Njk4NjY4ODcyNzg3NjEyODIyMzYyODExMzkxNjM4MDkyNzUwMjczNzkxMQoAAAAAAAAAYQAR6ZEOSb8am6giraofTNFOXN6N5etgegC1TsTKup7HNMtyZWPBn9WaDwPe+naJustyRgVE7K8umsX6h3Fa7UQMucbuFjDvPnERRKZI2wa7sihPcnTPvuU//O5QPMGkkgADAAIADX2rNYyNrapO+gBJp1sHQ2VVsQo2ghm7aA9wVxNJ13UBAzwbaHR0cHM6Ly9pZC50d2l0Y2gudHYvb2F1dGgyLflu6Eag/zG3tLd5CtZRYx9p1t34RovVSn/+uHFiYfcBAQA=',
		);

		const decoded = decodeMultiSig(multisig);
		expect(decoded).toEqual([
			{
				signature: '',
				signatureScheme: 'ZkLogin',
				pubKey: pk4,
				weight: 1,
			},
			{
				signature: parseSerializedSignature((await k1.signPersonalMessage(data)).signature)
					.signature,
				signatureScheme: k6.getKeyScheme(),
				pubKey: pk6,
				weight: 1,
			},
		]);
	});

	it('`decodeMultiSig()` should decode a multisig signature correctly', async () => {
		const pubkeyWeightPairs: PubkeyWeightPair[] = [
			{
				pubKey: pk1,
				weight: 1,
			},
			{
				pubKey: pk2,
				weight: 2,
			},
			{
				pubKey: pk3,
				weight: 3,
			},
		];

		const data = new Uint8Array([0, 0, 0, 5, 72, 101, 108, 108, 111]);

		const sig1 = await k1.signPersonalMessage(data);
		const sig2 = await k2.signPersonalMessage(data);
		const sig3 = await k3.signPersonalMessage(data);

		const multisig = combinePartialSigs(
			[sig1.signature, sig2.signature, sig3.signature],
			pubkeyWeightPairs,
			3,
		);

		const decoded = decodeMultiSig(multisig);
		expect(decoded).toEqual([
			{
				signature: parseSerializedSignature((await k1.signPersonalMessage(data)).signature)
					.signature,
				signatureScheme: k1.getKeyScheme(),
				pubKey: pk1,
				weight: 1,
			},
			{
				signature: parseSerializedSignature((await k2.signPersonalMessage(data)).signature)
					.signature,
				signatureScheme: k2.getKeyScheme(),
				pubKey: pk2,
				weight: 2,
			},
			{
				signature: parseSerializedSignature((await k3.signPersonalMessage(data)).signature)
					.signature,
				signatureScheme: k3.getKeyScheme(),
				pubKey: pk3,
				weight: 3,
			},
		]);
	});

	it('`decodeMultiSig()` should handle invalid parameters', async () => {
		const data = new Uint8Array([0, 0, 0, 5, 72, 101, 108, 108, 111]);

		const sig1 = await k1.signPersonalMessage(data);

		expect(() => decodeMultiSig(sig1.signature)).toThrowError(new Error('Invalid MultiSig flag'));

		expect(() => decodeMultiSig('')).toThrowError(new Error('Invalid MultiSig flag'));

		expect(() => decodeMultiSig('Invalid string')).toThrowError(new Error('Invalid MultiSig flag'));
	});
});
