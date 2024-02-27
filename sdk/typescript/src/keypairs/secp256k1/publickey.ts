// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/bcs';
import { secp256k1 } from '@noble/curves/secp256k1';
import { sha256 } from '@noble/hashes/sha256';

import { bytesEqual, PublicKey } from '../../cryptography/publickey.js';
import type { PublicKeyInitData } from '../../cryptography/publickey.js';
import { SIGNATURE_SCHEME_TO_FLAG } from '../../cryptography/signature-scheme.js';
import { parseSerializedSignature } from '../../cryptography/signature.js';

const SECP256K1_PUBLIC_KEY_SIZE = 33;

/**
 * A Secp256k1 public key
 */
export class Secp256k1PublicKey extends PublicKey {
	static SIZE = SECP256K1_PUBLIC_KEY_SIZE;
	private data: Uint8Array;

	/**
	 * Create a new Secp256k1PublicKey object
	 * @param value secp256k1 public key as buffer or base-64 encoded string
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

		if (this.data.length !== SECP256K1_PUBLIC_KEY_SIZE) {
			throw new Error(
				`Invalid public key input. Expected ${SECP256K1_PUBLIC_KEY_SIZE} bytes, got ${this.data.length}`,
			);
		}
	}

	/**
	 * Checks if two Secp256k1 public keys are equal
	 */
	override equals(publicKey: Secp256k1PublicKey): boolean {
		return super.equals(publicKey);
	}

	/**
	 * Return the byte array representation of the Secp256k1 public key
	 */
	toRawBytes(): Uint8Array {
		return this.data;
	}

	/**
	 * Return the Sui address associated with this Secp256k1 public key
	 */
	flag(): number {
		return SIGNATURE_SCHEME_TO_FLAG['Secp256k1'];
	}

	/**
	 * Verifies that the signature is valid for for the provided message
	 */
	async verify(message: Uint8Array, signature: Uint8Array | string): Promise<boolean> {
		let bytes;
		if (typeof signature === 'string') {
			const parsed = parseSerializedSignature(signature);
			if (parsed.signatureScheme !== 'Secp256k1') {
				throw new Error('Invalid signature scheme');
			}

			if (!bytesEqual(this.toRawBytes(), parsed.publicKey)) {
				throw new Error('Signature does not match public key');
			}

			bytes = parsed.signature;
		} else {
			bytes = signature;
		}

		return secp256k1.verify(
			secp256k1.Signature.fromCompact(bytes),
			sha256(message),
			this.toRawBytes(),
		);
	}
}
