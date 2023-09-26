// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { poseidonHash } from './poseidon.js';

const MAX_KEY_CLAIM_NAME_LENGTH = 32;
const MAX_KEY_CLAIM_VALUE_LENGTH = 115;
const MAX_AUD_VALUE_LENGTH = 145;
const PACK_WIDTH = 248;

// TODO: We need to rewrite this to not depend on Buffer.
export function toBufferBE(num: bigint, width: number) {
	const hex = num.toString(16);
	return Buffer.from(hex.padStart(width * 2, '0').slice(-width * 2), 'hex');
}

/**
 * Splits an array into chunks of size chunk_size. If the array is not evenly
 * divisible by chunk_size, the first chunk will be smaller than chunk_size.
 *
 * E.g., arrayChunk([1, 2, 3, 4, 5], 2) => [[1], [2, 3], [4, 5]]
 *
 * Note: Can be made more efficient by avoiding the reverse() calls.
 */
export function chunkArray<T>(array: T[], chunk_size: number): T[][] {
	const chunks = Array(Math.ceil(array.length / chunk_size));
	const revArray = array.reverse();
	for (let i = 0; i < chunks.length; i++) {
		chunks[i] = revArray.slice(i * chunk_size, (i + 1) * chunk_size).reverse();
	}
	return chunks.reverse();
}

function bytesBEToBigInt(bytes: number[]): bigint {
	const hex = bytes.map((b) => b.toString(16).padStart(2, '0')).join('');
	return BigInt('0x' + hex);
}

// hashes an ASCII string to a field element
export function hashASCIIStrToField(str: string, maxSize: number) {
	if (str.length > maxSize) {
		throw new Error(`String ${str} is longer than ${maxSize} chars`);
	}

	// Note: Padding with zeroes is safe because we are only using this function to map human-readable sequence of bytes.
	// So the ASCII values of those characters will never be zero (null character).
	const strPadded = str
		.padEnd(maxSize, String.fromCharCode(0))
		.split('')
		.map((c) => c.charCodeAt(0));

	const chunkSize = PACK_WIDTH / 8;
	const packed = chunkArray(strPadded, chunkSize).map((chunk) => bytesBEToBigInt(chunk));
	return poseidonHash(packed);
}

export function genAddressSeed(
	salt: bigint,
	name: string,
	value: string,
	aud: string,
	max_name_length = MAX_KEY_CLAIM_NAME_LENGTH,
	max_value_length = MAX_KEY_CLAIM_VALUE_LENGTH,
	max_aud_length = MAX_AUD_VALUE_LENGTH,
) {
	return poseidonHash([
		hashASCIIStrToField(name, max_name_length),
		hashASCIIStrToField(value, max_value_length),
		hashASCIIStrToField(aud, max_aud_length),
		poseidonHash([salt]),
	]);
}
