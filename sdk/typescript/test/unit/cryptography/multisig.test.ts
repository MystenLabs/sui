// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';
import {
	combinePartialSigs,
	decodeMultiSig,
	toMultiSigAddress,
} from '../../../src/cryptography/multisig';
import { Ed25519Keypair, Secp256k1Keypair, toSerializedSignature } from '../../../src';
import { blake2b } from '@noble/hashes/blake2b';

describe('multisig address and combine sigs', () => {
	// Address and combined multisig matches rust impl: fn multisig_serde_test()
	it('combines signature to multisig', () => {
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

		const data = new Uint8Array([0, 0, 0, 5, 72, 101, 108, 108, 111]);
		const digest = blake2b(data, { dkLen: 32 });

		const sig1 = {
			signature: k1.signData(digest),
			signatureScheme: k1.getKeyScheme(),
			pubKey: pk1,
		};

		const ser_sig1 = toSerializedSignature(sig1);

		const sig2 = {
			signature: k2.signData(digest),
			signatureScheme: k2.getKeyScheme(),
			pubKey: pk2,
		};

		const ser_sig2 = toSerializedSignature(sig2);
		expect(
			toMultiSigAddress(
				[
					{ pubKey: pk1, weight: 1 },
					{ pubKey: pk2, weight: 2 },
					{ pubKey: pk3, weight: 3 },
				],
				3,
			),
		).toEqual('0x37b048598ca569756146f4e8ea41666c657406db154a31f11bb5c1cbaf0b98d7');

		let multisig = combinePartialSigs(
			[ser_sig1, ser_sig2],
			[
				{ pubKey: pk1, weight: 1 },
				{ pubKey: pk2, weight: 2 },
				{ pubKey: pk3, weight: 3 },
			],
			3,
		);
		expect(multisig).toEqual(
			'AwIAvlJnUP0iJFZL+QTxkKC9FHZGwCa5I4TITHS/QDQ12q1sYW6SMt2Yp3PSNzsAay0Fp2MPVohqyyA02UtdQ2RNAQGH0eLk4ifl9h1I8Uc+4QlRYfJC21dUbP8aFaaRqiM/f32TKKg/4PSsGf9lFTGwKsHJYIMkDoqKwI8Xqr+3apQzAwADAFriILSy9l6XfBLt5hV5/1FwtsIsAGFow3tefGGvAYCDAQECHRUjB8a3Kw7QQYsOcM2A5/UpW42G9XItP1IT+9I5TzYCADtqJ7zOtqQtYqOo0CpvDXNlMhV3HeJDpjrASKGLWdopAwMA',
		);

		let decoded = decodeMultiSig(multisig);
		expect(decoded).toEqual([sig1, sig2]);
	});
});
