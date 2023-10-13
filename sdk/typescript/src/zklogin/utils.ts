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

export function toBigEndianBytes(num: bigint, width: number): Uint8Array {
	const hex = num.toString(16);
	const bytes = hexToBytes(hex.padStart(width * 2, '0').slice(-width * 2));

	const firstNonZeroIndex = findFirstNonZeroIndex(bytes);

	if (firstNonZeroIndex === -1) {
		return new Uint8Array([0]);
	}

	return bytes.slice(firstNonZeroIndex);
}
