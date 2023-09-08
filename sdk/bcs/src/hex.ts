// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export function fromHEX(hexStr: string): Uint8Array {
	const normalized = hexStr.startsWith('0x') ? hexStr.slice(2) : hexStr;
	const padded = normalized.length % 2 === 0 ? normalized : `0${normalized}}`;
	const intArr = padded.match(/.{2}/g)?.map((byte) => parseInt(byte, 16)) ?? [];

	return Uint8Array.from(intArr);
}

export function toHEX(bytes: Uint8Array): string {
	return bytes.reduce((str, byte) => str + byte.toString(16).padStart(2, '0'), '');
}
