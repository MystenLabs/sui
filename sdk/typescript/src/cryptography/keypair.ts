// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { PublicKey } from './publickey.js';
import type { SignatureScheme } from './signature.js';
import { IntentScope, messageWithIntent } from '../utils/intent.js';

export const PRIVATE_KEY_SIZE = 32;
export const LEGACY_PRIVATE_KEY_SIZE = 64;

export type ExportedKeypair = {
	schema: SignatureScheme;
	privateKey: string;
};

interface SignedMessage {
	bytes: Uint8Array;
	signature: Uint8Array;
}

/**
 * TODO: Document
 */
export abstract class Keypair {
	abstract sign(bytes: Uint8Array): Promise<Uint8Array>;

	async signWithIntent(bytes: Uint8Array, intent: IntentScope): Promise<SignedMessage> {
		const intentMessage = messageWithIntent(intent, bytes);
		const signature = await this.sign(intentMessage);

		return {
			bytes: intentMessage,
			signature,
		};
	}

	async signTransactionBlock(bytes: Uint8Array) {
		return this.signWithIntent(bytes, IntentScope.TransactionData);
	}

	async signMessage(bytes: Uint8Array) {
		return this.signWithIntent(bytes, IntentScope.PersonalMessage);
	}

	/**
	 * The public key for this keypair
	 */
	abstract getPublicKey(): PublicKey;

	/**
	 * Return the signature for the data.
	 * Prefer the async verion {@link sign}, as this method will be deprecated in a future release.
	 */
	abstract signData(data: Uint8Array): Uint8Array;

	/**
	 * Get the key scheme of the keypair: Secp256k1 or ED25519
	 */
	abstract getKeyScheme(): SignatureScheme;
	abstract export(): ExportedKeypair;
}
