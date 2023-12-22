// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB64 } from '@mysten/bcs';
import { blake2b } from '@noble/hashes/blake2b';
import { bech32 } from 'bech32';

import { bcs } from '../bcs/index.js';
import { IntentScope, messageWithIntent } from './intent.js';
import type { PublicKey } from './publickey.js';
import { SIGNATURE_SCHEME_TO_FLAG } from './signature-scheme.js';
import type { SignatureScheme } from './signature-scheme.js';
import type { SerializedSignature } from './signature.js';
import { toSerializedSignature } from './signature.js';

export const PRIVATE_KEY_SIZE = 32;
export const LEGACY_PRIVATE_KEY_SIZE = 64;
export const SUI_PRIVATE_KEY_PREFIX = 'suiprivkey';

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

	/**
	 * The plain bytes for the secret key for this keypair.
	 */
	abstract getSecretKeyBytes(): Uint8Array;
}

/**
 * This returns an exported keypair object, schema is the signature
 * scheme name, and the private key field is a Bech32 encoded string
 * of 33-byte `flag || private_key` that starts with `suiprivkey`.
 */
export abstract class Keypair extends BaseSigner {
	export(): ExportedKeypair {
		const plainBytes = this.getSecretKeyBytes();
		if (plainBytes.length != PRIVATE_KEY_SIZE) {
			throw new Error('Invwalid bytes length');
		}
		const flag = SIGNATURE_SCHEME_TO_FLAG[this.getKeyScheme()];
		const privKeyBytes = new Uint8Array(plainBytes.length + 1);
		privKeyBytes.set([flag]);
		privKeyBytes.set(plainBytes, 1);
		return {
			schema: this.getKeyScheme(),
			privateKey: bech32.encode(SUI_PRIVATE_KEY_PREFIX, bech32.toWords(privKeyBytes)),
		};
	}
}
