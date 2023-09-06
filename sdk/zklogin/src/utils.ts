// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { poseidonHash } from './poseidon.js';

type bit = 0 | 1;

const MAX_KEY_CLAIM_NAME_LENGTH = 40;
const MAX_KEY_CLAIM_VALUE_LENGTH = 100;
const MAX_AUD_VALUE_LENGTH = 150;
const PACK_WIDTH = 248;

// TODO: We need to rewrite this to not depend on Buffer.
export function toBufferBE(num: bigint, width: number) {
	const hex = num.toString(16);
	return Buffer.from(hex.padStart(width * 2, '0').slice(-width * 2), 'hex');
}

function bigintArrayToBitArray(arr: bigint[], intSize: number): bit[] {
	return arr.reduce((bitArray, n) => {
		const binaryString = n.toString(2).padStart(intSize, '0');
		const bitValues = binaryString.split('').map((bit) => (bit === '1' ? 1 : 0));
		return [...bitArray, ...bitValues];
	}, [] as bit[]);
}

function chunkArray<T>(arr: T[], chunkSize: number) {
	return Array.from({ length: Math.ceil(arr.length / chunkSize) }, (_, i) =>
		arr.slice(i * chunkSize, (i + 1) * chunkSize),
	);
}

/**
 * ConvertBase
 * 1. Converts each input element into exactly inWidth bits
 *     - Prefixing zeroes if needed
 * 2. Splits the resulting array into chunks of outWidth bits where
 *    the last chunk's size is <= outWidth bits.
 * 3. Converts each chunk into a bigint
 * 4. Returns a vector of size Math.ceil((inArr.length * inWidth) / outWidth)
 */
export function convertBase(inArr: bigint[], inWidth: number, outWidth: number): bigint[] {
	const bits = bigintArrayToBitArray(inArr, inWidth);
	const packed = chunkArray(bits, outWidth).map((chunk) => BigInt('0b' + chunk.join('')));
	return packed;
}

// hashes a stream of bigints to a field element
export function hashToField(input: bigint[], inWidth: number) {
	if (PACK_WIDTH % 8 !== 0) {
		throw new Error('PACK_WIDTH must be a multiple of 8');
	}
	const packed = convertBase(input, inWidth, PACK_WIDTH);
	return poseidonHash(packed);
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
		.map((c) => BigInt(c.charCodeAt(0)));

	return hashToField(strPadded, 8);
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
