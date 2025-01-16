// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { secp256r1 } from '@noble/curves/p256';
import { secp256k1 } from '@noble/curves/secp256k1';
import { ASN1Construction, ASN1TagClass, DERElement } from 'asn1-ts';

/** The total number of bits in the DER bit string for the uncompressed public key. */
export const DER_BIT_STRING_LENGTH = 520;

/** The total number of bytes corresponding to the DER bit string length. */
export const DER_BYTES_LENGTH = DER_BIT_STRING_LENGTH / 8;

// Reference Specifications:
// https://datatracker.ietf.org/doc/html/rfc5480#section-2.2
// https://www.secg.org/sec1-v2.pdf

/**
 * Converts an array of bits into a byte array.
 *
 * @param bitsArray - A `Uint8ClampedArray` representing the bits to convert.
 * @returns A `Uint8Array` containing the corresponding bytes.
 *
 * @throws {Error} If the input array does not have the expected length.
 */
function bitsToBytes(bitsArray: Uint8ClampedArray): Uint8Array {
	const bytes = new Uint8Array(DER_BYTES_LENGTH);
	for (let i = 0; i < DER_BIT_STRING_LENGTH; i++) {
		if (bitsArray[i] === 1) {
			bytes[Math.floor(i / 8)] |= 1 << (7 - (i % 8));
		}
	}
	return bytes;
}

export function publicKeyFromDER(derBytes: Uint8Array) {
	const encodedData: Uint8Array = derBytes;
	const derElement = new DERElement();
	derElement.fromBytes(encodedData);

	// Validate the ASN.1 structure of the public key
	if (
		!(
			derElement.tagClass === ASN1TagClass.universal &&
			derElement.construction === ASN1Construction.constructed
		)
	) {
		throw new Error('Unexpected ASN.1 structure');
	}

	const components = derElement.components;
	const publicKeyElement = components[1];

	if (!publicKeyElement) {
		throw new Error('Public Key not found in the DER structure');
	}

	return compressPublicKeyClamped(publicKeyElement.bitString);
}

export function getConcatenatedSignature(signature: Uint8Array, keyScheme: string) {
	if (!signature || signature.length === 0) {
		throw new Error('Invalid signature');
	}

	// Initialize a DERElement to parse the DER-encoded signature
	const derElement = new DERElement();
	derElement.fromBytes(signature);

	const [r, s] = derElement.toJSON() as [string, string];

	switch (keyScheme) {
		case 'Secp256k1':
			return new secp256k1.Signature(BigInt(r), BigInt(s)).normalizeS().toCompactRawBytes();
		case 'Secp256r1':
			return new secp256r1.Signature(BigInt(r), BigInt(s)).normalizeS().toCompactRawBytes();
		default:
			throw new Error('Unsupported key scheme');
	}
}

/**
 * Compresses an uncompressed public key into its compressed form.
 *
 * The uncompressed key must follow the DER bit string format as specified in [RFC 5480](https://datatracker.ietf.org/doc/html/rfc5480#section-2.2)
 * and [SEC 1: Elliptic Curve Cryptography](https://www.secg.org/sec1-v2.pdf).
 *
 * @param uncompressedKey - A `Uint8ClampedArray` representing the uncompressed public key bits.
 * @returns A `Uint8Array` containing the compressed public key.
 *
 * @throws {Error} If the uncompressed key has an unexpected length or does not start with the expected prefix.
 */
export function compressPublicKeyClamped(uncompressedKey: Uint8ClampedArray): Uint8Array {
	if (uncompressedKey.length !== DER_BIT_STRING_LENGTH) {
		throw new Error('Unexpected length for an uncompressed public key');
	}

	// Convert bits to bytes
	const uncompressedBytes = bitsToBytes(uncompressedKey);

	// Ensure the public key starts with the standard uncompressed prefix 0x04
	if (uncompressedBytes[0] !== 0x04) {
		throw new Error('Public key does not start with 0x04');
	}

	// Extract X-Coordinate (skip the first byte, which is the prefix 0x04)
	const xCoord = uncompressedBytes.slice(1, 33);

	// Determine parity byte for Y coordinate based on the last byte
	const yCoordLastByte = uncompressedBytes[64];
	const parityByte = yCoordLastByte % 2 === 0 ? 0x02 : 0x03;

	// Return the compressed public key consisting of the parity byte and X-coordinate
	return new Uint8Array([parityByte, ...xCoord]);
}
