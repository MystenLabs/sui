// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import { expect, test } from 'vitest';

import { generateNonce, generateRandomness } from '../src';

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
