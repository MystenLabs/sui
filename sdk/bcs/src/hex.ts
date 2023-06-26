// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export function fromHEX(hexStr: string): Uint8Array {
	// @ts-ignore
	let intArr = hexStr
		.replace('0x', '')
		.match(/.{1,2}/g)
		.map((byte) => parseInt(byte, 16));

	if (intArr === null) {
		throw new Error(`Unable to parse HEX: ${hexStr}`);
	}

	return Uint8Array.from(intArr);
}

export function toHEX(bytes: Uint8Array): string {
	return bytes.reduce((str, byte) => str + byte.toString(16).padStart(2, '0'), '');
}
