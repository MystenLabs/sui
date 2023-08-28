// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import { PublicKey, bytesEqual } from '../../../src/cryptography/publickey';
import { Ed25519Keypair, Ed25519PublicKey } from '../../../src/keypairs/ed25519';
import { Secp256k1Keypair } from '../../../src/keypairs/secp256k1';
import { Secp256r1Keypair } from '../../../src/keypairs/secp256r1';
import { toB64 } from '@mysten/bcs';
import { bcs } from '../../../src/bcs/index.js';
import { IntentScope } from '../../../src/cryptography/intent';
import { bytesToHex } from '@noble/hashes/utils';
import { blake2b } from '@noble/hashes/blake2b';
import { SUI_ADDRESS_LENGTH, normalizeSuiAddress } from '../../../src/utils/sui-types.js';

describe('Publickey', () => {
	let k1: Ed25519Keypair, pk1: Ed25519PublicKey,
		k2: Secp256k1Keypair, pk2: PublicKey,
		k3: Secp256r1Keypair, pk3: PublicKey;

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

	it('`bytesEqual()` should handle comparison correctly', async () => {
		expect(bytesEqual(pk2.toRawBytes(), pk3.toRawBytes())).toEqual(false);
		expect(bytesEqual(pk2.toRawBytes(), pk2.toRawBytes())).toEqual(true);
	});

	it('`equals()` should handle comparison correctly', async () => {
		expect(pk2.equals(pk3)).toEqual(false);
		expect(pk3.equals(pk3)).toEqual(true);
	});

	it('`toBase64()` should return a valid base-64 representation', async () => {
		expect(pk2.toBase64()).toEqual(toB64(pk2.toRawBytes()));
	});

	it('`toSuiPublicKey()` should return a valid sui representation', async () => {
		expect(pk2.toSuiPublicKey()).toEqual(toB64(pk2.toSuiBytes()));
	});

	it('`verifyWithIntent()` should correctly verify a signed message', async () => {
		const data = new Uint8Array([0, 0, 0, 5, 72, 101, 108, 108, 111]);

		const sig1 = await k1.signPersonalMessage(data);
		const sig2 = await k2.signTransactionBlock(data);

		expect(await pk1.verifyWithIntent(data, sig1.signature, IntentScope.PersonalMessage)).toEqual(
			false,
		);
		expect(await pk2.verifyWithIntent(data, sig2.signature, IntentScope.TransactionData)).toEqual(
			true,
		);
		expect(
			await pk1.verifyWithIntent(
				bcs.ser(['vector', 'u8'], data).toBytes(),
				sig1.signature,
				IntentScope.PersonalMessage,
			),
		).toEqual(true);
	});

	it('`verifyPersonalMessage()` should correctly verify a signed personal message', async () => {
		const data = new Uint8Array([0, 0, 0, 5, 72, 101, 108, 108, 111]);

		const sig1 = await k1.signPersonalMessage(data);
		const sig2 = await k2.signTransactionBlock(data);

		expect(await pk2.verifyPersonalMessage(data, sig2.signature)).toEqual(false);
		expect(await pk1.verifyPersonalMessage(data, sig1.signature)).toEqual(true);
	});

	it('`verifyTransactionBlock()` should correctly verify a signed transaction block', async () => {
		const data = new Uint8Array([0, 0, 0, 5, 72, 101, 108, 108, 111]);

		const sig1 = await k1.signPersonalMessage(data);
		const sig2 = await k2.signTransactionBlock(data);

		expect(await pk1.verifyTransactionBlock(data, sig1.signature)).toEqual(false);
		expect(await pk2.verifyTransactionBlock(data, sig2.signature)).toEqual(true);
	});

	it('`toSuiBytes()` should return the correct byte representation of the public key with the signature scheme flag', async () => {
		const pk1SuiBytes = new Uint8Array(pk1.toRawBytes().length + 1);
		pk1SuiBytes.set([0x00]);
		pk1SuiBytes.set(pk1.toRawBytes(), 1);

		expect(pk1.toSuiBytes()).toEqual(pk1SuiBytes);

		const pk2SuiBytes = new Uint8Array(pk2.toRawBytes().length + 1);
		pk2SuiBytes.set([0x01]);
		pk2SuiBytes.set(pk2.toRawBytes(), 1);

		expect(pk2.toSuiBytes()).toEqual(pk2SuiBytes);

		const pk3SuiBytes = new Uint8Array(pk3.toRawBytes().length + 1);
		pk3SuiBytes.set([0x02]);
		pk3SuiBytes.set(pk3.toRawBytes(), 1);

		expect(pk3.toSuiBytes()).toEqual(pk3SuiBytes);
	});

	it('`toSuiAddress()` should correctly return sui address associated with Ed25519 publickey', async () => {
		const pk1SuiAddress = normalizeSuiAddress(
			bytesToHex(blake2b(pk1.toSuiBytes(), { dkLen: 32 })).slice(0, SUI_ADDRESS_LENGTH * 2),
		);
		const pk2SuiAddress = normalizeSuiAddress(
			bytesToHex(blake2b(pk2.toSuiBytes(), { dkLen: 32 })).slice(0, SUI_ADDRESS_LENGTH * 2),
		);
		const pk3SuiAddress = normalizeSuiAddress(
			bytesToHex(blake2b(pk3.toSuiBytes(), { dkLen: 32 })).slice(0, SUI_ADDRESS_LENGTH * 2),
		);
		expect(k1.toSuiAddress()).toEqual(pk1SuiAddress);
		expect(k2.toSuiAddress()).toEqual(pk2SuiAddress);
		expect(k3.toSuiAddress()).toEqual(pk3SuiAddress);
	});
});
