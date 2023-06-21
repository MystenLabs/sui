// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';
import {
	BCS,
	getSuiMoveConfig,
	fromB58,
	toB58,
	fromB64,
	toB64,
	fromHEX,
	toHEX,
} from './../src/index';

describe('BCS: Encodings', () => {
	it('should de/ser hex, base58 and base64', () => {
		const bcs = new BCS(getSuiMoveConfig());

		expect(bcs.de('u8', 'AA==', 'base64')).toEqual(0);
		expect(bcs.de('u8', '00', 'hex')).toEqual(0);
		expect(bcs.de('u8', '1', 'base58')).toEqual(0);

		const STR = 'this is a test string';
		const str = bcs.ser('string', STR);

		expect(bcs.de('string', fromB58(str.toString('base58')), 'base58')).toEqual(STR);
		expect(bcs.de('string', fromB64(str.toString('base64')), 'base64')).toEqual(STR);
		expect(bcs.de('string', fromHEX(str.toString('hex')), 'hex')).toEqual(STR);
	});

	it('should de/ser native encoding types', () => {
		const bcs = new BCS(getSuiMoveConfig());

		bcs.registerStructType('TestStruct', {
			hex: BCS.HEX,
			base58: BCS.BASE58,
			base64: BCS.BASE64,
		});

		let hex_str = toHEX(new Uint8Array([1, 2, 3, 4, 5, 6]));
		let b58_str = toB58(new Uint8Array([1, 2, 3, 4, 5, 6]));
		let b64_str = toB64(new Uint8Array([1, 2, 3, 4, 5, 6]));

		let serialized = bcs.ser('TestStruct', {
			hex: hex_str,
			base58: b58_str,
			base64: b64_str,
		});

		let deserialized = bcs.de('TestStruct', serialized.toBytes());

		expect(deserialized.hex).toEqual(hex_str);
		expect(deserialized.base58).toEqual(b58_str);
		expect(deserialized.base64).toEqual(b64_str);
	});
});
