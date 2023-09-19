// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB58, toB58 } from './b58';
import { fromB64, toB64 } from './b64';
import { fromHEX, toHEX } from './hex';
import { Encoding } from './types';

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
			return toB58(data);
		case 'base64':
			return toB64(data);
		case 'hex':
			return toHEX(data);
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
			return fromB58(data);
		case 'base64':
			return fromB64(data);
		case 'hex':
			return fromHEX(data);
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
