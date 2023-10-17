// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { PublicKey } from '@mysten/sui.js/cryptography';
import { toBigEndianBytes } from '@mysten/sui.js/zklogin';
import { randomBytes } from '@noble/hashes/utils';
import { base64url } from 'jose';

import { poseidonHash } from './poseidon.js';
import { toBigIntBE } from './utils.js';

export const NONCE_LENGTH = 27;

export function generateRandomness() {
	// Once Node 20 enters LTS, we can just use crypto.getRandomValues(new Uint8Array(16)), but until then this improves compatibility:
	return String(toBigIntBE(randomBytes(16)));
}

export function generateNonce(publicKey: PublicKey, maxEpoch: number, randomness: bigint | string) {
	const publicKeyBytes = toBigIntBE(publicKey.toSuiBytes());
	const eph_public_key_0 = publicKeyBytes / 2n ** 128n;
	const eph_public_key_1 = publicKeyBytes % 2n ** 128n;
	const bigNum = poseidonHash([eph_public_key_0, eph_public_key_1, maxEpoch, BigInt(randomness)]);
	const Z = toBigEndianBytes(bigNum, 20);
	const nonce = base64url.encode(Z);
	if (nonce.length !== NONCE_LENGTH) {
		throw new Error(`Length of nonce ${nonce} (${nonce.length}) is not equal to ${NONCE_LENGTH}`);
	}
	return nonce;
}
