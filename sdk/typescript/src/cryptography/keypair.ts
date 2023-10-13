// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB64 } from '@mysten/bcs';
import { blake2b } from '@noble/hashes/blake2b';

import { bcs } from '../bcs/index.js';
import { IntentScope, messageWithIntent } from './intent.js';
import type { PublicKey } from './publickey.js';
import type { SignatureScheme } from './signature-scheme.js';
import type { SerializedSignature } from './signature.js';
import { toSerializedSignature } from './signature.js';

export const PRIVATE_KEY_SIZE = 32;
export const LEGACY_PRIVATE_KEY_SIZE = 64;

export type ExportedKeypair = {
	schema: SignatureScheme;
	privateKey: string;
};

export interface SignatureWithBytes {
	bytes: string;
	signature: SerializedSignature;
}

/**
 * TODO: Document
 */
export abstract class BaseSigner {
	abstract sign(bytes: Uint8Array): Promise<Uint8Array>;
	/**
	 * Sign messages with a specific intent. By combining the message bytes with the intent before hashing and signing,
	 * it ensures that a signed message is tied to a specific purpose and domain separator is provided
	 */
	async signWithIntent(bytes: Uint8Array, intent: IntentScope): Promise<SignatureWithBytes> {
		const intentMessage = messageWithIntent(intent, bytes);
		const digest = blake2b(intentMessage, { dkLen: 32 });

		const signature = toSerializedSignature({
			signature: await this.sign(digest),
			signatureScheme: this.getKeyScheme(),
			publicKey: this.getPublicKey(),
		});

		return {
			signature,
			bytes: toB64(bytes),
		};
	}
	/**
	 * Signs provided transaction block by calling `signWithIntent()` with a `TransactionData` provided as intent scope
	 */
	async signTransactionBlock(bytes: Uint8Array) {
		return this.signWithIntent(bytes, IntentScope.TransactionData);
	}
	/**
	 * Signs provided personal message by calling `signWithIntent()` with a `PersonalMessage` provided as intent scope
	 */
	async signPersonalMessage(bytes: Uint8Array) {
		return this.signWithIntent(
			bcs.vector(bcs.u8()).serialize(bytes).toBytes(),
			IntentScope.PersonalMessage,
		);
	}

	toSuiAddress(): string {
		return this.getPublicKey().toSuiAddress();
	}

	/**
	 * Return the signature for the data.
	 * Prefer the async version {@link sign}, as this method will be deprecated in a future release.
	 */
	abstract signData(data: Uint8Array): Uint8Array;

	/**
	 * Get the key scheme of the keypair: Secp256k1 or ED25519
	 */
	abstract getKeyScheme(): SignatureScheme;

	/**
	 * The public key for this keypair
	 */
	abstract getPublicKey(): PublicKey;
}

/**
 * TODO: Document
 */
export abstract class Keypair extends BaseSigner {
	abstract export(): ExportedKeypair;
}
