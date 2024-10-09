// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { secp256k1 } from '@noble/curves/secp256k1';
import { DERElement } from 'asn1-ts';

export const DER_BIT_STRING_LENGTH = 520;
export const DER_BYTES_LENGTH = DER_BIT_STRING_LENGTH / 8;

// https://datatracker.ietf.org/doc/html/rfc5480#section-2.2
// https://www.secg.org/sec1-v2.pdf
function bitsToBytes(bitsArray: Uint8ClampedArray) {
	const bytes = new Uint8Array(DER_BYTES_LENGTH);
	for (let i = 0; i < DER_BIT_STRING_LENGTH; i++) {
		if (bitsArray[i] === 1) {
			bytes[Math.floor(i / 8)] |= 1 << (7 - (i % 8));
		}
	}
	return bytes;
}

export function compressPublicKeyClamped(uncompressedKey: Uint8ClampedArray) {
	if (uncompressedKey.length !== DER_BIT_STRING_LENGTH) {
		throw new Error('Unexpected length for an uncompressed public key');
	}

	// Convert bits to bytes
	const uncompressedBytes = bitsToBytes(uncompressedKey);
	//console.log("Uncompressed Bytes:", uncompressedBytes);

	// Check if the first byte is 0x04
	if (uncompressedBytes[0] !== 0x04) {
		throw new Error('Public key does not start with 0x04');
	}

	// Extract X-Coordinate (skip the first byte, which should be 0x04)
	const xCoord = uncompressedBytes.slice(1, 33);

	// Determine parity byte for y coordinate
	const yCoordLastByte = uncompressedBytes[64];
	const parityByte = yCoordLastByte % 2 === 0 ? 0x02 : 0x03;

	return new Uint8Array([parityByte, ...xCoord]);
}

// creates signature consumable by Sui 'toSerializedSignature' call
export function getConcatenatedSignature(signature: Uint8Array) {
	if (!signature || signature.length === 0) {
		throw new Error('Invalid signature');
	}

	// start processing signature
	// populate concatenatedSignature with [r,s] from DER signature
	const derElement = new DERElement();
	derElement.fromBytes(signature);

	const derJsonData = derElement.toJSON() as {
		value: string;
	}[];

	const newR = derJsonData[0];
	const newS = derJsonData[1];

	const secp256k1Signature = new secp256k1.Signature(BigInt(String(newR)), BigInt(String(newS)));

	// secp256k1_normalize_s_compact
	return secp256k1Signature.normalizeS().toCompactRawBytes();
}
