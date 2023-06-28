// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { PublicKey } from './publickey.js';
import type { SignatureScheme } from './signature.js';

export const PRIVATE_KEY_SIZE = 32;
export const LEGACY_PRIVATE_KEY_SIZE = 64;

export type ExportedKeypair = {
	schema: SignatureScheme;
	privateKey: string;
};

/**
 * A keypair used for signing transactions.
 */
export interface Keypair {
	/**
	 * The public key for this keypair
	 */
	getPublicKey(): PublicKey;

	/**
	 * Return the signature for the data
	 */
	signData(data: Uint8Array): Uint8Array;

	/**
	 * Get the key scheme of the keypair: Secp256k1 or ED25519
	 */
	getKeyScheme(): SignatureScheme;

	export(): ExportedKeypair;
}
