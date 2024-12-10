// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromBase64, toBase64 } from '@mysten/bcs';
import { secp256r1 } from '@noble/curves/p256';
import { sha256 } from '@noble/hashes/sha256';

import { PasskeyAuthenticator } from '../../bcs/bcs.js';
import { bytesEqual, PublicKey } from '../../cryptography/publickey.js';
import type { PublicKeyInitData } from '../../cryptography/publickey.js';
import { SIGNATURE_SCHEME_TO_FLAG } from '../../cryptography/signature-scheme.js';

export const PASSKEY_PUBLIC_KEY_SIZE = 33;
export const PASSKEY_UNCOMPRESSED_PUBLIC_KEY_SIZE = 65;
export const PASSKEY_SIGNATURE_SIZE = 64;
/** Fixed DER header for secp256r1 SubjectPublicKeyInfo
DER structure for P-256 SPKI:
30 -- SEQUENCE
  59 -- length (89 bytes)
  30 -- SEQUENCE
    13 -- length (19 bytes)
    06 -- OBJECT IDENTIFIER
      07 -- length
      2A 86 48 CE 3D 02 01 -- id-ecPublicKey
    06 -- OBJECT IDENTIFIER
      08 -- length
      2A 86 48 CE 3D 03 01 07 -- secp256r1/prime256v1
  03 -- BIT STRING
    42 -- length (66 bytes)
    00 -- padding
	===== above bytes are considered header =====
    04 || x || y -- uncompressed point (65 bytes: 0x04 || 32-byte x || 32-byte y)
*/
export const SECP256R1_SPKI_HEADER = new Uint8Array([
	0x30,
	0x59, // SEQUENCE, length 89
	0x30,
	0x13, // SEQUENCE, length 19
	0x06,
	0x07, // OID, length 7
	0x2a,
	0x86,
	0x48,
	0xce,
	0x3d,
	0x02,
	0x01, // OID: 1.2.840.10045.2.1 (ecPublicKey)
	0x06,
	0x08, // OID, length 8
	0x2a,
	0x86,
	0x48,
	0xce,
	0x3d,
	0x03,
	0x01,
	0x07, // OID: 1.2.840.10045.3.1.7 (prime256v1/secp256r1)
	0x03,
	0x42, // BIT STRING, length 66
	0x00, // no unused bits
] as const);

/**
 * A passkey public key
 */
export class PasskeyPublicKey extends PublicKey {
	static SIZE = PASSKEY_PUBLIC_KEY_SIZE;
	private data: Uint8Array;

	/**
	 * Create a new PasskeyPublicKey object
	 * @param value passkey public key as buffer or base-64 encoded string
	 */
	constructor(value: PublicKeyInitData) {
		super();

		if (typeof value === 'string') {
			this.data = fromBase64(value);
		} else if (value instanceof Uint8Array) {
			this.data = value;
		} else {
			this.data = Uint8Array.from(value);
		}

		if (this.data.length !== PASSKEY_PUBLIC_KEY_SIZE) {
			throw new Error(
				`Invalid public key input. Expected ${PASSKEY_PUBLIC_KEY_SIZE} bytes, got ${this.data.length}`,
			);
		}
	}

	/**
	 * Checks if two passkey public keys are equal
	 */
	override equals(publicKey: PasskeyPublicKey): boolean {
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
		return SIGNATURE_SCHEME_TO_FLAG['Passkey'];
	}

	/**
	 * Verifies that the signature is valid for for the provided message
	 */
	async verify(message: Uint8Array, signature: Uint8Array | string): Promise<boolean> {
		const parsed = parseSerializedPasskeySignature(signature);
		const clientDataJSON = JSON.parse(parsed.clientDataJson);

		if (clientDataJSON.type !== 'webauthn.get') {
			return false;
		}

		// parse challenge from base64 url
		const parsedChallenge = fromBase64(
			clientDataJSON.challenge.replace(/-/g, '+').replace(/_/g, '/'),
		);
		if (!bytesEqual(message, parsedChallenge)) {
			return false;
		}

		const pk = parsed.userSignature.slice(1 + PASSKEY_SIGNATURE_SIZE);
		if (!bytesEqual(this.toRawBytes(), pk)) {
			return false;
		}

		const payload = new Uint8Array([...parsed.authenticatorData, ...sha256(parsed.clientDataJson)]);
		const sig = parsed.userSignature.slice(1, PASSKEY_SIGNATURE_SIZE + 1);
		return secp256r1.verify(sig, sha256(payload), pk);
	}
}

/**
 * Parses a DER SubjectPublicKeyInfo into an uncompressed public key. This also verifies
 * that the curve used is P-256 (secp256r1).
 *
 * @param data: DER SubjectPublicKeyInfo
 * @returns uncompressed public key (`0x04 || x || y`)
 */
export function parseDerSPKI(derBytes: Uint8Array): Uint8Array {
	// Verify length and header bytes are expected
	if (derBytes.length !== SECP256R1_SPKI_HEADER.length + PASSKEY_UNCOMPRESSED_PUBLIC_KEY_SIZE) {
		throw new Error('Invalid DER length');
	}
	for (let i = 0; i < SECP256R1_SPKI_HEADER.length; i++) {
		if (derBytes[i] !== SECP256R1_SPKI_HEADER[i]) {
			throw new Error('Invalid spki header');
		}
	}

	if (derBytes[SECP256R1_SPKI_HEADER.length] !== 0x04) {
		throw new Error('Invalid point marker');
	}

	// Returns the last 65 bytes `04 || x || y`
	return derBytes.slice(SECP256R1_SPKI_HEADER.length);
}

/**
 * Parse signature from bytes or base64 string into the following fields.
 */
export function parseSerializedPasskeySignature(signature: Uint8Array | string) {
	const bytes = typeof signature === 'string' ? fromBase64(signature) : signature;

	if (bytes[0] !== SIGNATURE_SCHEME_TO_FLAG.Passkey) {
		throw new Error('Invalid signature scheme');
	}
	const dec = PasskeyAuthenticator.parse(bytes.slice(1));
	return {
		signatureScheme: 'Passkey' as const,
		serializedSignature: toBase64(bytes),
		signature: bytes,
		authenticatorData: dec.authenticatorData,
		clientDataJson: dec.clientDataJson,
		userSignature: new Uint8Array(dec.userSignature),
		publicKey: new Uint8Array(dec.userSignature.slice(1 + PASSKEY_SIGNATURE_SIZE)),
	};
}
