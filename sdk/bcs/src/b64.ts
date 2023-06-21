// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/*\
|*|  Base64 / binary data / UTF-8 strings utilities
|*|  https://developer.mozilla.org/en-US/docs/Web/JavaScript/Base64_encoding_and_decoding
\*/

/* Array of bytes to Base64 string decoding */

function b64ToUint6(nChr: number) {
	return nChr > 64 && nChr < 91
		? nChr - 65
		: nChr > 96 && nChr < 123
		? nChr - 71
		: nChr > 47 && nChr < 58
		? nChr + 4
		: nChr === 43
		? 62
		: nChr === 47
		? 63
		: 0;
}

export function fromB64(sBase64: string, nBlocksSize?: number): Uint8Array {
	var sB64Enc = sBase64.replace(/[^A-Za-z0-9+/]/g, ''),
		nInLen = sB64Enc.length,
		nOutLen = nBlocksSize
			? Math.ceil(((nInLen * 3 + 1) >> 2) / nBlocksSize) * nBlocksSize
			: (nInLen * 3 + 1) >> 2,
		taBytes = new Uint8Array(nOutLen);

	for (var nMod3, nMod4, nUint24 = 0, nOutIdx = 0, nInIdx = 0; nInIdx < nInLen; nInIdx++) {
		nMod4 = nInIdx & 3;
		nUint24 |= b64ToUint6(sB64Enc.charCodeAt(nInIdx)) << (6 * (3 - nMod4));
		if (nMod4 === 3 || nInLen - nInIdx === 1) {
			for (nMod3 = 0; nMod3 < 3 && nOutIdx < nOutLen; nMod3++, nOutIdx++) {
				taBytes[nOutIdx] = (nUint24 >>> ((16 >>> nMod3) & 24)) & 255;
			}
			nUint24 = 0;
		}
	}

	return taBytes;
}

/* Base64 string to array encoding */

function uint6ToB64(nUint6: number) {
	return nUint6 < 26
		? nUint6 + 65
		: nUint6 < 52
		? nUint6 + 71
		: nUint6 < 62
		? nUint6 - 4
		: nUint6 === 62
		? 43
		: nUint6 === 63
		? 47
		: 65;
}

export function toB64(aBytes: Uint8Array): string {
	var nMod3 = 2,
		sB64Enc = '';

	for (var nLen = aBytes.length, nUint24 = 0, nIdx = 0; nIdx < nLen; nIdx++) {
		nMod3 = nIdx % 3;
		nUint24 |= aBytes[nIdx] << ((16 >>> nMod3) & 24);
		if (nMod3 === 2 || aBytes.length - nIdx === 1) {
			sB64Enc += String.fromCodePoint(
				uint6ToB64((nUint24 >>> 18) & 63),
				uint6ToB64((nUint24 >>> 12) & 63),
				uint6ToB64((nUint24 >>> 6) & 63),
				uint6ToB64(nUint24 & 63),
			);
			nUint24 = 0;
		}
	}

	return (
		sB64Enc.slice(0, sB64Enc.length - 2 + nMod3) + (nMod3 === 2 ? '' : nMod3 === 1 ? '=' : '==')
	);
}
