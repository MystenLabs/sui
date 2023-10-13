// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Helper utility: write number as an ULEB array.
// Original code is taken from: https://www.npmjs.com/package/uleb128 (no longer exists)
export function ulebEncode(num: number): number[] {
	let arr = [];
	let len = 0;

	if (num === 0) {
		return [0];
	}

	while (num > 0) {
		arr[len] = num & 0x7f;
		if ((num >>= 7)) {
			arr[len] |= 0x80;
		}
		len += 1;
	}

	return arr;
}

// Helper utility: decode ULEB as an array of numbers.
// Original code is taken from: https://www.npmjs.com/package/uleb128 (no longer exists)
export function ulebDecode(arr: number[] | Uint8Array): {
	value: number;
	length: number;
} {
	let total = 0;
	let shift = 0;
	let len = 0;

	// eslint-disable-next-line no-constant-condition
	while (true) {
		let byte = arr[len];
		len += 1;
		total |= (byte & 0x7f) << shift;
		if ((byte & 0x80) === 0) {
			break;
		}
		shift += 7;
	}

	return {
		value: total,
		length: len,
	};
}
