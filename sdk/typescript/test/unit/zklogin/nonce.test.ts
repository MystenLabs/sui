// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { expect, test } from 'vitest';

import { Ed25519Keypair } from '../../../src/keypairs/ed25519';
import { generateNonce, generateRandomness } from '../../../src/zklogin';

test('can generate using `generateRandomness`', () => {
	const kp = Ed25519Keypair.fromSecretKey(new Uint8Array(32));
	const randomness = generateRandomness();
	expect(generateNonce(kp.getPublicKey(), 0, randomness)).toBeTypeOf('string');
});

test('can generate using a bigint', () => {
	const kp = Ed25519Keypair.fromSecretKey(new Uint8Array(32));
	const randomness = 0n;
	expect(generateNonce(kp.getPublicKey(), 0, randomness)).toBeTypeOf('string');
});
