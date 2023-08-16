// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { poseidonHash } from './poseidon.js';

type bit = 0 | 1;

const MAX_KEY_CLAIM_NAME_LENGTH = 40;
const MAX_KEY_CLAIM_VALUE_LENGTH = 256;
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

// Pack into an array of chunks each outWidth bits
function pack(inArr: bigint[], inWidth: number, outWidth: number, outCount: number): bigint[] {
	const bits = bigintArrayToBitArray(inArr, inWidth);
	const extraBits = bits.length % outWidth === 0 ? 0 : outWidth - (bits.length % outWidth);
	const bitsPadded = bits.concat(Array(extraBits).fill(0));
	if (bitsPadded.length % outWidth !== 0) {
		throw new Error('Invalid logic');
	}
	const packed = chunkArray(bitsPadded, outWidth).map((chunk) => BigInt('0b' + chunk.join('')));
	return packed.concat(Array(outCount - packed.length).fill(0));
}

function mapToField(input: bigint[], inWidth: number) {
	if (PACK_WIDTH % 8 !== 0) {
		throw new Error('PACK_WIDTH must be a multiple of 8');
	}
	const numElements = Math.ceil((input.length * inWidth) / PACK_WIDTH);
	const packed = pack(input, inWidth, PACK_WIDTH, numElements);
	return poseidonHash(packed);
}

// Pads a stream of bytes and maps it to a field element
function mapBytesToField(str: string, maxSize: number) {
	if (str.length > maxSize) {
		throw new Error(`String ${str} is longer than ${maxSize} chars`);
	}

	// Note: Padding with zeroes is safe because we are only using this function to map human-readable sequence of bytes.
	// So the ASCII values of those characters will never be zero (null character).
	const strPadded = str
		.padEnd(maxSize, String.fromCharCode(0))
		.split('')
		.map((c) => BigInt(c.charCodeAt(0)));

	return mapToField(strPadded, 8);
}

export function genAddressSeed(pin: bigint, name: string, value: string) {
	return poseidonHash([
		mapBytesToField(name, MAX_KEY_CLAIM_NAME_LENGTH),
		mapBytesToField(value, MAX_KEY_CLAIM_VALUE_LENGTH),
		poseidonHash([pin]),
	]);
}
