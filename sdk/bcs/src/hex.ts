// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export function fromHex(hexStr: string): Uint8Array {
	const normalized = hexStr.startsWith('0x') ? hexStr.slice(2) : hexStr;
	const padded = normalized.length % 2 === 0 ? normalized : `0${normalized}}`;
	const intArr = padded.match(/.{2}/g)?.map((byte) => parseInt(byte, 16)) ?? [];

	return Uint8Array.from(intArr);
}

export function toHex(bytes: Uint8Array): string {
	return bytes.reduce((str, byte) => str + byte.toString(16).padStart(2, '0'), '');
}

/** @deprecated use toHex instead */
export const toHEX = toHex;

/** @deprecated use fromHex instead */
export const fromHEX = fromHex;
