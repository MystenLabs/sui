// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromBase64 } from '@mysten/bcs';
import { blake2b } from '@noble/hashes/blake2b';
import { bytesToHex } from '@noble/hashes/utils';
import { beforeAll, describe, expect, it } from 'vitest';

import { bcs } from '../../../src/bcs/index.js';
import { messageWithIntent } from '../../../src/cryptography/intent';
import { PublicKey } from '../../../src/cryptography/publickey';
import { SIGNATURE_SCHEME_TO_FLAG } from '../../../src/cryptography/signature-scheme.js';
import { parseSerializedSignature } from '../../../src/cryptography/signature.js';
import { Ed25519Keypair, Ed25519PublicKey } from '../../../src/keypairs/ed25519';
import { Secp256k1Keypair } from '../../../src/keypairs/secp256k1';
import { Secp256r1Keypair } from '../../../src/keypairs/secp256r1';
import {
	MAX_SIGNER_IN_MULTISIG,
	MultiSigPublicKey,
	MultiSigStruct,
	parsePartialSignatures,
} from '../../../src/multisig/publickey';
import { normalizeSuiAddress } from '../../../src/utils/sui-types.js';

describe('Publickey', () => {
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

	it('`fromPublicKeys()` should create multisig correctly', async () => {
		const multiSigPublicKey = MultiSigPublicKey.fromPublicKeys({
			threshold: 3,
			publicKeys: [
				{ publicKey: pk1, weight: 1 },
				{ publicKey: pk2, weight: 2 },
				{ publicKey: pk3, weight: 3 },
			],
		});

		expect(multiSigPublicKey).toBeInstanceOf(MultiSigPublicKey);
		expect(multiSigPublicKey.getPublicKeys()).toEqual([
			{ publicKey: pk1, weight: 1 },
			{ publicKey: pk2, weight: 2 },
			{ publicKey: pk3, weight: 3 },
		]);
	});

	it('`fromPublicKeys()` should handle invalid parameters', async () => {
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
					{ publicKey: pk4, weight: 1 },
					{ publicKey: pk5, weight: 2 },
					{ publicKey: pk6, weight: 3 },
					{ publicKey: pk7, weight: 1 },
					{ publicKey: pk8, weight: 2 },
					{ publicKey: pk9, weight: 3 },
					{ publicKey: pk10, weight: 1 },
					{ publicKey: pk11, weight: 2 },
				],
			}),
		).toThrowError(new Error(`Max number of signers in a multisig is ${MAX_SIGNER_IN_MULTISIG}`));
	});

	it('`constructor()` should create multisig correctly', async () => {
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

		const multisig = multiSigPublicKey.combinePartialSignatures([sig1.signature, sig2.signature]);
		const parsed = parseSerializedSignature(multisig);

		if (parsed.signatureScheme !== 'MultiSig') {
			throw new Error('Expected signature scheme to be MultiSig');
		}
		const publicKey = new MultiSigPublicKey(parsed.multisig!.multisig_pk);
		expect(publicKey).toBeInstanceOf(MultiSigPublicKey);
		expect(publicKey.getPublicKeys()).toEqual([
			{ publicKey: pk1, weight: 1 },
			{ publicKey: pk2, weight: 2 },
			{ publicKey: pk3, weight: 3 },
		]);
		expect(publicKey.equals(multiSigPublicKey)).toEqual(true);
	});

	it('`equals()` should handle multisig comparison correctly', async () => {
		const multiSigPublicKey1 = MultiSigPublicKey.fromPublicKeys({
			threshold: 3,
			publicKeys: [
				{ publicKey: pk1, weight: 1 },
				{ publicKey: pk2, weight: 2 },
				{ publicKey: pk3, weight: 3 },
			],
		});

		const multiSigPublicKey2 = MultiSigPublicKey.fromPublicKeys({
			threshold: 4,
			publicKeys: [
				{ publicKey: pk1, weight: 1 },
				{ publicKey: pk2, weight: 2 },
				{ publicKey: pk3, weight: 3 },
			],
		});

		const multiSigPublicKey3 = MultiSigPublicKey.fromPublicKeys({
			threshold: 3,
			publicKeys: [
				{ publicKey: pk1, weight: 1 },
				{ publicKey: pk2, weight: 2 },
			],
		});

		const multiSigPublicKey4 = MultiSigPublicKey.fromPublicKeys({
			threshold: 3,
			publicKeys: [
				{ publicKey: pk1, weight: 1 },
				{ publicKey: pk2, weight: 2 },
				{ publicKey: pk3, weight: 3 },
			],
		});

		expect(multiSigPublicKey1.equals(multiSigPublicKey2)).toEqual(false);
		expect(multiSigPublicKey1.equals(multiSigPublicKey3)).toEqual(false);
		expect(multiSigPublicKey1.equals(multiSigPublicKey4)).toEqual(true);
	});

	it('`toRawBytes()` should return correct array representation', async () => {
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

		const multisig = multiSigPublicKey.combinePartialSignatures([sig1.signature, sig2.signature]);
		const rawBytes = fromBase64(multisig).slice(134);

		expect(multiSigPublicKey.toRawBytes()).toEqual(rawBytes);
		expect(multiSigPublicKey.toRawBytes()).toEqual(
			new Uint8Array([
				3, 0, 90, 226, 32, 180, 178, 246, 94, 151, 124, 18, 237, 230, 21, 121, 255, 81, 112, 182,
				194, 44, 0, 97, 104, 195, 123, 94, 124, 97, 175, 1, 128, 131, 1, 1, 2, 29, 21, 35, 7, 198,
				183, 43, 14, 208, 65, 139, 14, 112, 205, 128, 231, 245, 41, 91, 141, 134, 245, 114, 45, 63,
				82, 19, 251, 210, 57, 79, 54, 2, 2, 2, 39, 50, 43, 58, 137, 26, 10, 40, 13, 107, 193, 251,
				44, 187, 35, 210, 143, 84, 144, 111, 214, 64, 127, 95, 116, 31, 109, 239, 87, 98, 96, 154,
				3, 3, 0,
			]),
		);
	});

	it('`getPublicKeys()` should return correct publickeys', async () => {
		const multiSigPublicKey = MultiSigPublicKey.fromPublicKeys({
			threshold: 3,
			publicKeys: [
				{ publicKey: pk1, weight: 1 },
				{ publicKey: pk2, weight: 2 },
				{ publicKey: pk3, weight: 3 },
			],
		});

		expect(multiSigPublicKey.getPublicKeys()).toEqual([
			{ publicKey: pk1, weight: 1 },
			{ publicKey: pk2, weight: 2 },
			{ publicKey: pk3, weight: 3 },
		]);
	});

	it('`toSuiAddress()` should return correct sui address associated with multisig publickey', async () => {
		const multiSigPublicKey = MultiSigPublicKey.fromPublicKeys({
			threshold: 3,
			publicKeys: [
				{ publicKey: pk1, weight: 1 },
				{ publicKey: pk2, weight: 2 },
				{ publicKey: pk3, weight: 3 },
			],
		});

		const maxLength = 1 + (64 + 1) * MAX_SIGNER_IN_MULTISIG + 2;
		const tmp = new Uint8Array(maxLength);
		tmp.set([0x03]);
		tmp.set(bcs.U16.serialize(3).toBytes(), 1);
		let i = 3;
		for (const { publicKey, weight } of multiSigPublicKey.getPublicKeys()) {
			const bytes = publicKey.toSuiBytes();
			tmp.set(bytes, i);
			i += bytes.length;
			tmp.set([weight], i++);
		}
		const multisigSuiAddress = normalizeSuiAddress(
			bytesToHex(blake2b(tmp.slice(0, i), { dkLen: 32 })),
		);

		expect(multiSigPublicKey.toSuiAddress()).toEqual(multisigSuiAddress);
		expect(multiSigPublicKey.toSuiAddress()).toEqual(
			'0x8ee027fe556a3f6c0a23df64f090d2429fec0bb21f55594783476e81de2dec27',
		);
	});

	it('`flag()` should return correct signature scheme', async () => {
		const multiSigPublicKey = MultiSigPublicKey.fromPublicKeys({
			threshold: 3,
			publicKeys: [
				{ publicKey: pk1, weight: 1 },
				{ publicKey: pk2, weight: 2 },
				{ publicKey: pk3, weight: 3 },
			],
		});

		expect(multiSigPublicKey.flag()).toEqual(3);
		expect(multiSigPublicKey.flag()).toEqual(SIGNATURE_SCHEME_TO_FLAG['MultiSig']);
	});

	it('`verify()` should verify the signature correctly', async () => {
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

		let multisig = multiSigPublicKey.combinePartialSignatures([sig1.signature, sig2.signature]);

		const intentMessage = messageWithIntent(
			'PersonalMessage',
			bcs.vector(bcs.U8).serialize(data).toBytes(),
		);
		const digest = blake2b(intentMessage, { dkLen: 32 });

		expect(await multiSigPublicKey.verify(digest, multisig)).toEqual(true);
	});

	it('`verify()` should handle invalid signature schemes', async () => {
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

		multiSigPublicKey.combinePartialSignatures([sig1.signature, sig2.signature]);

		const intentMessage = messageWithIntent(
			'PersonalMessage',
			bcs.vector(bcs.U8).serialize(data).toBytes(),
		);
		const digest = blake2b(intentMessage, { dkLen: 32 });

		expect(async () => await multiSigPublicKey.verify(digest, sig1.signature)).rejects.toThrow(
			new Error('Invalid signature scheme'),
		);
	});

	it('`combinePartialSignatures()` should combine with different signatures into a single multisig correctly', async () => {
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

		const multisig = multiSigPublicKey.combinePartialSignatures([sig1.signature, sig2.signature]);

		expect(multisig).toEqual(
			'AwIANe9gJJmT5m1UvpV8Hj7nOyif76rS5Zgg1bi7VApts+KwtSc2Bg8WJ6LBfGnZKugrOqtQsk5d2Q+IMRLD4hYmBQFYlrlXc01/ZSdgwSD3eGEdm6kxwtOwAvTWdb2wNZP2Hnkgrh+indYN4s2Qd99iYCz+xsY6aT5lpOBsDZb2x9LyAwADAFriILSy9l6XfBLt5hV5/1FwtsIsAGFow3tefGGvAYCDAQECHRUjB8a3Kw7QQYsOcM2A5/UpW42G9XItP1IT+9I5TzYCAgInMis6iRoKKA1rwfssuyPSj1SQb9ZAf190H23vV2JgmgMDAA==',
		);

		const decoded = bcs.MultiSig.parse(fromBase64(multisig).slice(1));

		expect(decoded).toEqual({
			bitmap: 3,
			sigs: [
				{
					$kind: 'ED25519',
					ED25519: Array.from(
						parseSerializedSignature((await k1.signPersonalMessage(data)).signature).signature!,
					),
				},
				{
					$kind: 'Secp256k1',
					Secp256k1: Array.from(
						parseSerializedSignature((await k2.signPersonalMessage(data)).signature).signature!,
					),
				},
			],
			multisig_pk: bcs.MultiSigPublicKey.parse(multiSigPublicKey.toRawBytes()),
		});
	});

	it('`combinePartialSignatures()` should handle invalid parameters', async () => {
		const multiSigPublicKey = MultiSigPublicKey.fromPublicKeys({
			threshold: 3,
			publicKeys: [
				{ publicKey: pk1, weight: 1 },
				{ publicKey: pk2, weight: 2 },
			],
		});

		const data = new Uint8Array([0, 0, 0, 5, 72, 101, 108, 108, 111]);

		const sig1 = await k1.signPersonalMessage(data);
		const sig2 = await k2.signPersonalMessage(data);
		const sig3 = await k3.signPersonalMessage(data);

		expect(() =>
			multiSigPublicKey.combinePartialSignatures([sig1.signature, sig3.signature]),
		).toThrowError(new Error('Received signature from unknown public key'));

		expect(() =>
			multiSigPublicKey.combinePartialSignatures([sig1.signature, sig1.signature]),
		).toThrowError(new Error('Received multiple signatures from the same public key'));

		const multisig = multiSigPublicKey.combinePartialSignatures([sig1.signature, sig2.signature]);

		expect(() =>
			multiSigPublicKey.combinePartialSignatures([multisig, sig1.signature]),
		).toThrowError(new Error('MultiSig is not supported inside MultiSig'));
	});

	it('`parsePartialSignatures()` should parse serialized signatures correctly', async () => {
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

		const multisig = multiSigPublicKey.combinePartialSignatures([sig1.signature, sig2.signature]);

		const bytes = fromBase64(multisig);
		const multiSigStruct: MultiSigStruct = bcs.MultiSig.parse(bytes.slice(1));

		const parsedPartialSignatures = parsePartialSignatures(multiSigStruct);

		expect(parsedPartialSignatures).toEqual([
			{
				signature: parseSerializedSignature((await k1.signPersonalMessage(data)).signature)
					.signature,
				signatureScheme: k1.getKeyScheme(),
				publicKey: pk1,
				weight: 1,
			},
			{
				signature: parseSerializedSignature((await k2.signPersonalMessage(data)).signature)
					.signature,
				signatureScheme: k2.getKeyScheme(),
				publicKey: pk2,
				weight: 2,
			},
		]);
	});
});
