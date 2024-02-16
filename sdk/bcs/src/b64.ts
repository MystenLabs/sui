// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export function fromB64(base64String: string): Uint8Array {
	return Uint8Array.from(atob(base64String), (char) => char.charCodeAt(0));
}

const CHUNK_SIZE = 8192;
export function toB64(bytes: Uint8Array): string {
	// Special-case the simple case for speed's sake.
	if (bytes.length < CHUNK_SIZE) {
		return btoa(String.fromCharCode(...bytes));
	}

	let output = '';
	for (var i = 0; i < bytes.length; i += CHUNK_SIZE) {
		const chunk = bytes.slice(i, i + CHUNK_SIZE);
		output += String.fromCharCode(...chunk);
	}

	return btoa(output);
}
