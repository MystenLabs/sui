// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/bcs';
import { secp256r1 } from '@noble/curves/p256';
import { sha256 } from '@noble/hashes/sha256';

import { bytesEqual, PublicKey } from '../../cryptography/publickey.js';
import type { PublicKeyInitData } from '../../cryptography/publickey.js';
import { SIGNATURE_SCHEME_TO_FLAG } from '../../cryptography/signature-scheme.js';
import type { SerializedSignature } from '../../cryptography/signature.js';
import { parseSerializedSignature } from '../../cryptography/signature.js';

const SECP256R1_PUBLIC_KEY_SIZE = 33;

/**
 * A Secp256r1 public key
 */
export class Secp256r1PublicKey extends PublicKey {
	static SIZE = SECP256R1_PUBLIC_KEY_SIZE;
	private data: Uint8Array;

	/**
	 * Create a new Secp256r1PublicKey object
	 * @param value secp256r1 public key as buffer or base-64 encoded string
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

		if (this.data.length !== SECP256R1_PUBLIC_KEY_SIZE) {
			throw new Error(
				`Invalid public key input. Expected ${SECP256R1_PUBLIC_KEY_SIZE} bytes, got ${this.data.length}`,
			);
		}
	}

	/**
	 * Checks if two Secp256r1 public keys are equal
	 */
	override equals(publicKey: Secp256r1PublicKey): boolean {
		return super.equals(publicKey);
	}

	/**
	 * Return the byte array representation of the Secp256r1 public key
	 */
	toRawBytes(): Uint8Array {
		return this.data;
	}

	/**
	 * Return the Sui address associated with this Secp256r1 public key
	 */
	flag(): number {
		return SIGNATURE_SCHEME_TO_FLAG['Secp256r1'];
	}

	/**
	 * Verifies that the signature is valid for for the provided message
	 */
	async verify(message: Uint8Array, signature: Uint8Array | SerializedSignature): Promise<boolean> {
		let bytes;
		if (typeof signature === 'string') {
			const parsed = parseSerializedSignature(signature);
			if (parsed.signatureScheme !== 'Secp256r1') {
				throw new Error('Invalid signature scheme');
			}

			if (!bytesEqual(this.toRawBytes(), parsed.publicKey)) {
				throw new Error('Signature does not match public key');
			}

			bytes = parsed.signature;
		} else {
			bytes = signature;
		}

		return secp256r1.verify(
			secp256r1.Signature.fromCompact(bytes),
			sha256(message),
			this.toRawBytes(),
		);
	}
}
