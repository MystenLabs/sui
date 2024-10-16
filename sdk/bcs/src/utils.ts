// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromBase58, toBase58 } from './b58.js';
import { fromBase64, toBase64 } from './b64.js';
import { fromHex, toHex } from './hex.js';
import type { Encoding } from './types.js';

/**
 * Encode data with either `hex` or `base64`.
 *
 * @param {Uint8Array} data Data to encode.
 * @param {String} encoding Encoding to use: base64 or hex
 * @return {String} Encoded value.
 */
export function encodeStr(data: Uint8Array, encoding: Encoding): string {
	switch (encoding) {
		case 'base58':
			return toBase58(data);
		case 'base64':
			return toBase64(data);
		case 'hex':
			return toHex(data);
		default:
			throw new Error('Unsupported encoding, supported values are: base64, hex');
	}
}

/**
 * Decode either `base64` or `hex` data.
 *
 * @param {String} data Data to encode.
 * @param {String} encoding Encoding to use: base64 or hex
 * @return {Uint8Array} Encoded value.
 */
export function decodeStr(data: string, encoding: Encoding): Uint8Array {
	switch (encoding) {
		case 'base58':
			return fromBase58(data);
		case 'base64':
			return fromBase64(data);
		case 'hex':
			return fromHex(data);
		default:
			throw new Error('Unsupported encoding, supported values are: base64, hex');
	}
}

export function splitGenericParameters(
	str: string,
	genericSeparators: [string, string] = ['<', '>'],
) {
	const [left, right] = genericSeparators;
	const tok = [];
	let word = '';
	let nestedAngleBrackets = 0;

	for (let i = 0; i < str.length; i++) {
		const char = str[i];
		if (char === left) {
			nestedAngleBrackets++;
		}
		if (char === right) {
			nestedAngleBrackets--;
		}
		if (nestedAngleBrackets === 0 && char === ',') {
			tok.push(word.trim());
			word = '';
			continue;
		}
		word += char;
	}

	tok.push(word.trim());

	return tok;
}
