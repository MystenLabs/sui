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
import { MultiSigPublicKey } from '../../../src/multisig/publickey';

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
		pk3: PublicKey;

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

		k1 = Ed25519Keypair.fromSecretKey(secret_key_k1);
		pk1 = k1.getPublicKey();

		k2 = Secp256k1Keypair.fromSecretKey(secret_key_k1);
		pk2 = k2.getPublicKey();

		k3 = Secp256r1Keypair.fromSecretKey(secret_key_r1);
		pk3 = k3.getPublicKey();
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
