// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/bcs';
import { beforeAll, describe, expect, it } from 'vitest';

import { ExportedKeypair } from '../../../src/cryptography/keypair';
import { PublicKey } from '../../../src/cryptography/publickey';
import { parseSerializedSignature } from '../../../src/cryptography/signature';
import {
	fromExportedKeypair,
	publicKeyFromSerialized,
	toParsedSignaturePubkeyPair,
	toSingleSignaturePubkeyPair,
} from '../../../src/cryptography/utils';
import { Ed25519Keypair, Ed25519PublicKey } from '../../../src/keypairs/ed25519';
import { Secp256k1Keypair } from '../../../src/keypairs/secp256k1';
import { Secp256r1Keypair } from '../../../src/keypairs/secp256r1';
import { MultiSigPublicKey } from '../../../src/multisig';

describe('Utils', () => {
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

	it('`toParsedSignaturePubkeyPair()` should parse signature correctly', async () => {
		const publicKey = MultiSigPublicKey.fromPublicKeys({
			publicKeys: [
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
			],
			threshold: 3,
		});

		const data = new Uint8Array([0, 0, 0, 5, 72, 101, 108, 108, 111]);

		const sig1 = await k1.signPersonalMessage(data);
		const sig2 = await k2.signPersonalMessage(data);

		const parsed = toParsedSignaturePubkeyPair(sig1.signature);

		expect(parsed[0].signatureScheme).toEqual(k1.getKeyScheme());
		expect(parsed[0].pubKey).toEqual(pk1);
		expect(parsed[0].signature).toEqual(parseSerializedSignature(sig1.signature).signature);

		const multisig = publicKey.combinePartialSignatures([sig1.signature, sig2.signature]);

		const parsedMultisig = toParsedSignaturePubkeyPair(multisig);

		expect(parsedMultisig).toEqual([
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

	it('`toSingleSignaturePubkeyPair()` should parse single signature publickey pair', async () => {
		const data = new Uint8Array([0, 0, 0, 5, 72, 101, 108, 108, 111]);

		const sig1 = await k1.signPersonalMessage(data);

		const parsed = toSingleSignaturePubkeyPair(sig1.signature);

		expect(parsed.signatureScheme).toEqual(k1.getKeyScheme());
		expect(parsed.pubKey).toEqual(pk1);
		expect(parsed.signature).toEqual(parseSerializedSignature(sig1.signature).signature);
	});

	it('`toSingleSignaturePubkeyPair()` should handle multisig', async () => {
		const publicKey = MultiSigPublicKey.fromPublicKeys({
			publicKeys: [
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
			],
			threshold: 3,
		});

		const data = new Uint8Array([0, 0, 0, 5, 72, 101, 108, 108, 111]);

		const sig1 = await k1.signPersonalMessage(data);
		const sig2 = await k2.signPersonalMessage(data);

		const parsed = toSingleSignaturePubkeyPair(sig1.signature);

		expect(parsed.signatureScheme).toEqual(k1.getKeyScheme());
		expect(parsed.pubKey).toEqual(pk1);
		expect(parsed.signature).toEqual(parseSerializedSignature(sig1.signature).signature);

		const multisig = publicKey.combinePartialSignatures([sig1.signature, sig2.signature]);

		expect(() => toSingleSignaturePubkeyPair(multisig)).toThrowError(
			new Error('Expected a single signature'),
		);
	});

	it('`publicKeyFromSerialized()` should return publickey correctly', async () => {
		expect(publicKeyFromSerialized(k1.getKeyScheme(), pk1.toBase64())).toEqual(pk1);
		expect(publicKeyFromSerialized(k2.getKeyScheme(), pk2.toBase64())).toEqual(pk2);
	});

	it('`publicKeyFromSerialized()` should handle unsupported schemes', async () => {
		expect(() => publicKeyFromSerialized('MultiSig', pk1.toBase64())).toThrowError(
			new Error('Unknown public key schema'),
		);
	});

	it('`fromExportedKeypair()` should return keypair correctly', async () => {
		const TEST_CASE = 'AN0JMHpDum3BhrVwnkylH0/HGRHBQ/fO/8+MYOawO8j6';

		const raw = fromB64(TEST_CASE);
		const imported = Ed25519Keypair.fromSecretKey(raw.slice(1));
		const exported = imported.export();

		const exportedKeypair: ExportedKeypair = {
			schema: exported.schema,
			privateKey: exported.privateKey,
		};

		const keypair = fromExportedKeypair(exportedKeypair);

		expect(keypair.getPublicKey()).toEqual(imported.getPublicKey());
		expect(keypair.getKeyScheme()).toEqual(imported.getKeyScheme());
	});

	it('`fromExportedKeypair()` should handle unsupported schemes', async () => {
		const TEST_CASE = 'AN0JMHpDum3BhrVwnkylH0/HGRHBQ/fO/8+MYOawO8j6';

		const raw = fromB64(TEST_CASE);
		const imported = Ed25519Keypair.fromSecretKey(raw.slice(1));
		const exported = imported.export();

		const exportedKeypair: ExportedKeypair = {
			schema: 'MultiSig',
			privateKey: exported.privateKey,
		};

		expect(() => fromExportedKeypair(exportedKeypair)).toThrowError(
			new Error('Invalid keypair schema MultiSig'),
		);
	});
});
