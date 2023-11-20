// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { hexToBytes } from '@noble/hashes/utils';

function findFirstNonZeroIndex(bytes: Uint8Array) {
	for (let i = 0; i < bytes.length; i++) {
		if (bytes[i] !== 0) {
			return i;
		}
	}

	return -1;
}

// Derive bytearray from num where the bytearray is padded to the left with 0s to the specified width.
export function toPaddedBigEndianBytes(num: bigint, width: number): Uint8Array {
	const hex = num.toString(16);
	return hexToBytes(hex.padStart(width * 2, '0').slice(-width * 2));
}

// Derive bytearray from num where the bytearray is not padded with 0.
export function toBigEndianBytes(num: bigint, width: number): Uint8Array {
	const bytes = toPaddedBigEndianBytes(num, width);

	const firstNonZeroIndex = findFirstNonZeroIndex(bytes);

	if (firstNonZeroIndex === -1) {
		return new Uint8Array([0]);
	}

	return bytes.slice(firstNonZeroIndex);
}
