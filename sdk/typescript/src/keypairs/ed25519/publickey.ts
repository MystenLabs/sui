// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/bcs';
import nacl from 'tweetnacl';

import type { PublicKeyInitData } from '../../cryptography/publickey.js';
import { bytesEqual, PublicKey } from '../../cryptography/publickey.js';
import { SIGNATURE_SCHEME_TO_FLAG } from '../../cryptography/signature-scheme.js';
import { parseSerializedSignature } from '../../cryptography/signature.js';

const PUBLIC_KEY_SIZE = 32;

/**
 * An Ed25519 public key
 */
export class Ed25519PublicKey extends PublicKey {
	static SIZE = PUBLIC_KEY_SIZE;
	private data: Uint8Array;

	/**
	 * Create a new Ed25519PublicKey object
	 * @param value ed25519 public key as buffer or base-64 encoded string
	 */
	constructor(value: PublicKeyInitData) {
		super();

		if (typeof value === 'string') {
			this.data = fromB64(value);
		} else if (value instanceof Uint8Array) {
			this.data = value;
		} else {
			this.data = Uint8Array.from(value);
		}

		if (this.data.length !== PUBLIC_KEY_SIZE) {
			throw new Error(
				`Invalid public key input. Expected ${PUBLIC_KEY_SIZE} bytes, got ${this.data.length}`,
			);
		}
	}

	/**
	 * Checks if two Ed25519 public keys are equal
	 */
	override equals(publicKey: Ed25519PublicKey): boolean {
		return super.equals(publicKey);
	}

	/**
	 * Return the byte array representation of the Ed25519 public key
	 */
	toRawBytes(): Uint8Array {
		return this.data;
	}

	/**
	 * Return the Sui address associated with this Ed25519 public key
	 */
	flag(): number {
		return SIGNATURE_SCHEME_TO_FLAG['ED25519'];
	}

	/**
	 * Verifies that the signature is valid for for the provided message
	 */
	async verify(message: Uint8Array, signature: Uint8Array | string): Promise<boolean> {
		let bytes;
		if (typeof signature === 'string') {
			const parsed = parseSerializedSignature(signature);
			if (parsed.signatureScheme !== 'ED25519') {
				throw new Error('Invalid signature scheme');
			}

			if (!bytesEqual(this.toRawBytes(), parsed.publicKey)) {
				throw new Error('Signature does not match public key');
			}

			bytes = parsed.signature;
		} else {
			bytes = signature;
		}

		return nacl.sign.detached.verify(message, bytes, this.toRawBytes());
	}
}
