// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

function base64UrlCharTo6Bits(base64UrlChar: string): number[] {
	if (base64UrlChar.length !== 1) {
		throw new Error('Invalid base64Url character: ' + base64UrlChar);
	}

	// Define the base64URL character set
	const base64UrlCharacterSet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_';

	// Find the index of the input character in the base64URL character set
	const index = base64UrlCharacterSet.indexOf(base64UrlChar);

	if (index === -1) {
		throw new Error('Invalid base64Url character: ' + base64UrlChar);
	}

	// Convert the index to a 6-bit binary string
	const binaryString = index.toString(2).padStart(6, '0');

	// Convert the binary string to an array of bits
	const bits = Array.from(binaryString).map(Number);

	return bits;
}

function base64UrlStringToBitVector(base64UrlString: string) {
	let bitVector: number[] = [];
	for (let i = 0; i < base64UrlString.length; i++) {
		const base64UrlChar = base64UrlString.charAt(i);
		const bits = base64UrlCharTo6Bits(base64UrlChar);
		bitVector = bitVector.concat(bits);
	}
	return bitVector;
}

function decodeBase64URL(s: string, i: number): string {
	if (s.length < 2) {
		throw new Error(`Input (s = ${s}) is not tightly packed because s.length < 2`);
	}
	let bits = base64UrlStringToBitVector(s);

	const firstCharOffset = i % 4;
	if (firstCharOffset === 0) {
		// skip
	} else if (firstCharOffset === 1) {
		bits = bits.slice(2);
	} else if (firstCharOffset === 2) {
		bits = bits.slice(4);
	} else {
		// (offset == 3)
		throw new Error(`Input (s = ${s}) is not tightly packed because i%4 = 3 (i = ${i}))`);
	}

	const lastCharOffset = (i + s.length - 1) % 4;
	if (lastCharOffset === 3) {
		// skip
	} else if (lastCharOffset === 2) {
		bits = bits.slice(0, bits.length - 2);
	} else if (lastCharOffset === 1) {
		bits = bits.slice(0, bits.length - 4);
	} else {
		// (offset == 0)
		throw new Error(
			`Input (s = ${s}) is not tightly packed because (i + s.length - 1)%4 = 0 (i = ${i}))`,
		);
	}

	if (bits.length % 8 !== 0) {
		throw new Error(`We should never reach here...`);
	}

	const bytes = new Uint8Array(Math.floor(bits.length / 8));
	let currentByteIndex = 0;
	for (let i = 0; i < bits.length; i += 8) {
		const bitChunk = bits.slice(i, i + 8);

		// Convert the 8-bit chunk to a byte and add it to the bytes array
		const byte = parseInt(bitChunk.join(''), 2);
		bytes[currentByteIndex++] = byte;
	}
	return new TextDecoder().decode(bytes);
}

function verifyExtendedClaim(claim: string) {
	// Last character of each extracted_claim must be '}' or ','
	if (!(claim.slice(-1) === '}' || claim.slice(-1) === ',')) {
		throw new Error('Invalid claim');
	}

	// A hack to parse the JSON key-value pair.. but it should work
	const json = JSON.parse('{' + claim.slice(0, -1) + '}');
	if (Object.keys(json).length !== 1) {
		throw new Error('Invalid claim');
	}
	const key = Object.keys(json)[0];
	return [key, json[key]];
}

export type Claim = {
	value: string;
	indexMod4: number;
};

export function extractClaimValue<R>(claim: Claim, claimName: string): R {
	const extendedClaim = decodeBase64URL(claim.value, claim.indexMod4);
	const [name, value] = verifyExtendedClaim(extendedClaim);
	if (name !== claimName) {
		throw new Error(`Invalid field name: found ${name} expected ${claimName}`);
	}
	return value;
}
