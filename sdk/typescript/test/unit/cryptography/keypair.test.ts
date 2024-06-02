// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB64 } from '@mysten/bcs';
import { beforeAll, describe, expect, it } from 'vitest';

import { bcs } from '../../../src/bcs/index.js';
import { PublicKey } from '../../../src/cryptography/publickey';
import { Ed25519Keypair, Ed25519PublicKey } from '../../../src/keypairs/ed25519';
import { Secp256k1Keypair } from '../../../src/keypairs/secp256k1';
import { Secp256r1Keypair } from '../../../src/keypairs/secp256r1';

describe('Keypair', () => {
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

	it('`signWithIntent()` should return the correct signature', async () => {
		const data = new Uint8Array([0, 0, 0, 5, 72, 101, 108, 108, 111]);
		const bytes = bcs.vector(bcs.U8).serialize(data).toBytes();

		const sig1 = await k1.signWithIntent(bytes, 'PersonalMessage');
		const sig2 = await k2.signWithIntent(data, 'TransactionData');
		const sig3 = await k3.signWithIntent(bytes, 'PersonalMessage');

		expect(sig1.bytes).toEqual(toB64(bytes));
		expect(sig1.bytes).toEqual('CQAAAAVIZWxsbw==');
		expect(sig1.signature).toEqual(
			'ADXvYCSZk+ZtVL6VfB4+5zson++q0uWYINW4u1QKbbPisLUnNgYPFieiwXxp2SroKzqrULJOXdkPiDESw+IWJgVa4iC0svZel3wS7eYVef9RcLbCLABhaMN7XnxhrwGAgw==',
		);

		expect(sig2.bytes).toEqual(toB64(data));
		expect(sig2.bytes).toEqual('AAAABUhlbGxv');
		expect(sig2.signature).toEqual(
			'AdF5r9uq1AygqXu+WrGoe+fSVU2ld1F3lxTAcj9Uh38lR9j6trumZ7VPvIuzsnIlDqeiPKzo98KSVXy+AVraiKsCHRUjB8a3Kw7QQYsOcM2A5/UpW42G9XItP1IT+9I5TzY=',
		);

		expect(sig3.bytes).toEqual(toB64(bytes));
		expect(sig3.bytes).toEqual('CQAAAAVIZWxsbw==');
		expect(sig3.signature).toEqual(
			'Apd48/4qVHSja5u2i7ZxobPL6iTLulNIuCxbd5GhfWVvcd69k9BtIqpFGMYXYyn7zapyvnJbtUZsF2ILc7Rp/X0CJzIrOokaCigNa8H7LLsj0o9UkG/WQH9fdB9t71diYJo=',
		);
	});

	it('`signTransaction()` should correctly sign a transaction block', async () => {
		const data = new Uint8Array([0, 0, 0, 5, 72, 101, 108, 108, 111]);

		const sig1 = await k1.signTransaction(data);
		const sig2 = await k2.signTransaction(data);
		const sig3 = await k3.signTransaction(data);

		expect(sig1.bytes).toEqual(toB64(data));
		expect(sig1.bytes).toEqual('AAAABUhlbGxv');
		expect(sig1.signature).toEqual(
			'AKu3E+/SrcDpRHQYYljHAxkcUBzXHwkXtdi7X57rYd3f/VeckrWHfU6GgFiwFLEvLexGWNYPGSJKL12VJTCzFQpa4iC0svZel3wS7eYVef9RcLbCLABhaMN7XnxhrwGAgw==',
		);

		expect(sig2.bytes).toEqual(toB64(data));
		expect(sig2.bytes).toEqual('AAAABUhlbGxv');
		expect(sig2.signature).toEqual(
			'AdF5r9uq1AygqXu+WrGoe+fSVU2ld1F3lxTAcj9Uh38lR9j6trumZ7VPvIuzsnIlDqeiPKzo98KSVXy+AVraiKsCHRUjB8a3Kw7QQYsOcM2A5/UpW42G9XItP1IT+9I5TzY=',
		);

		expect(sig3.bytes).toEqual(toB64(data));
		expect(sig3.bytes).toEqual('AAAABUhlbGxv');
		expect(sig3.signature).toEqual(
			'AvKS25z99kTnsHe70qf2Dd9+Lz0DHTzM7cKFrMF47Z2RNy6qSFzOV87thExeKqug6VvEFiaqYhplx3fsT/rgk9kCJzIrOokaCigNa8H7LLsj0o9UkG/WQH9fdB9t71diYJo=',
		);
	});

	it('`signPersonalMessage()` should correctly sign a personal message', async () => {
		const data = new Uint8Array([0, 0, 0, 5, 72, 101, 108, 108, 111]);

		const sig1 = await k1.signPersonalMessage(data);
		const sig2 = await k2.signPersonalMessage(data);
		const sig3 = await k3.signPersonalMessage(data);

		expect(sig1.bytes).toEqual(toB64(data));
		expect(sig1.bytes).toEqual('AAAABUhlbGxv');
		expect(sig1.signature).toEqual(
			'ADXvYCSZk+ZtVL6VfB4+5zson++q0uWYINW4u1QKbbPisLUnNgYPFieiwXxp2SroKzqrULJOXdkPiDESw+IWJgVa4iC0svZel3wS7eYVef9RcLbCLABhaMN7XnxhrwGAgw==',
		);

		expect(sig2.bytes).toEqual(toB64(data));
		expect(sig2.bytes).toEqual('AAAABUhlbGxv');
		expect(sig2.signature).toEqual(
			'AViWuVdzTX9lJ2DBIPd4YR2bqTHC07AC9NZ1vbA1k/YeeSCuH6Kd1g3izZB332JgLP7GxjppPmWk4GwNlvbH0vICHRUjB8a3Kw7QQYsOcM2A5/UpW42G9XItP1IT+9I5TzY=',
		);

		expect(sig3.bytes).toEqual(toB64(data));
		expect(sig3.bytes).toEqual('AAAABUhlbGxv');
		expect(sig3.signature).toEqual(
			'Apd48/4qVHSja5u2i7ZxobPL6iTLulNIuCxbd5GhfWVvcd69k9BtIqpFGMYXYyn7zapyvnJbtUZsF2ILc7Rp/X0CJzIrOokaCigNa8H7LLsj0o9UkG/WQH9fdB9t71diYJo=',
		);
	});

	it('`toSuiAddress()` should return a valid sui address', async () => {
		expect(k1.toSuiAddress()).toEqual(pk1.toSuiAddress());
		expect(k1.toSuiAddress()).toEqual(
			'0xafedf3bc60bd296aa6830d7c48ca44e0f7a32478ae4bd7b9a6ac1dc81ff7b29b',
		);

		expect(k2.toSuiAddress()).toEqual(pk2.toSuiAddress());
		expect(k2.toSuiAddress()).toEqual(
			'0x7e4f9a35bf3b5383802d990956d6f3c93e6184ebbbcf0820c124ab3a59ef77ac',
		);

		expect(k3.toSuiAddress()).toEqual(pk3.toSuiAddress());
		expect(k3.toSuiAddress()).toEqual(
			'0x318f591092f10b67a81963954fb9539ea3919444417726be4e1b95ce44fe2fc0',
		);
	});
});
