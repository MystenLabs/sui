// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from 'vitest';
import { BcsType, BcsBuilder } from '../src/builder.js';
import { toHEX, BcsWriter, BcsReader, toB58 } from '../src';

describe('BcsBuilder', () => {
	describe('base types', () => {
		testType('true', BcsBuilder.bool(), true, '01');
		testType('false', BcsBuilder.bool(), false, '00');
		testType('uleb128 0', BcsBuilder.uleb128(), 0, '00');
		testType('uleb128 1', BcsBuilder.uleb128(), 1, '01');
		testType('uleb128 127', BcsBuilder.uleb128(), 127, '7f');
		testType('uleb128 128', BcsBuilder.uleb128(), 128, '8001');
		testType('uleb128 255', BcsBuilder.uleb128(), 255, 'ff01');
		testType('uleb128 256', BcsBuilder.uleb128(), 256, '8002');
		testType('uleb128 16383', BcsBuilder.uleb128(), 16383, 'ff7f');
		testType('uleb128 16384', BcsBuilder.uleb128(), 16384, '808001');
		testType('uleb128 2097151', BcsBuilder.uleb128(), 2097151, 'ffff7f');
		testType('uleb128 2097152', BcsBuilder.uleb128(), 2097152, '80808001');
		testType('uleb128 268435455', BcsBuilder.uleb128(), 268435455, 'ffffff7f');
		testType('uleb128 268435456', BcsBuilder.uleb128(), 268435456, '8080808001');
		testType('u8 0', BcsBuilder.u8(), 0, '00');
		testType('u8 1', BcsBuilder.u8(), 1, '01');
		testType('u8 255', BcsBuilder.u8(), 255, 'ff');
		testType('u16 0', BcsBuilder.u16(), 0, '0000');
		testType('u16 1', BcsBuilder.u16(), 1, '0100');
		testType('u16 255', BcsBuilder.u16(), 255, 'ff00');
		testType('u16 256', BcsBuilder.u16(), 256, '0001');
		testType('u16 65535', BcsBuilder.u16(), 65535, 'ffff');
		testType('u32 0', BcsBuilder.u32(), 0, '00000000');
		testType('u32 1', BcsBuilder.u32(), 1, '01000000');
		testType('u32 255', BcsBuilder.u32(), 255, 'ff000000');
		testType('u32 256', BcsBuilder.u32(), 256, '00010000');
		testType('u32 65535', BcsBuilder.u32(), 65535, 'ffff0000');
		testType('u32 65536', BcsBuilder.u32(), 65536, '00000100');
		testType('u32 16777215', BcsBuilder.u32(), 16777215, 'ffffff00');
		testType('u32 16777216', BcsBuilder.u32(), 16777216, '00000001');
		testType('u32 4294967295', BcsBuilder.u32(), 4294967295, 'ffffffff');
		testType('u64 0', BcsBuilder.u64(), 0, '0000000000000000', 0n);
		testType('u64 1', BcsBuilder.u64(), 1, '0100000000000000', 1n);
		testType('u64 255', BcsBuilder.u64(), 255n, 'ff00000000000000');
		testType('u64 256', BcsBuilder.u64(), 256n, '0001000000000000');
		testType('u64 65535', BcsBuilder.u64(), 65535n, 'ffff000000000000');
		testType('u64 65536', BcsBuilder.u64(), 65536n, '0000010000000000');
		testType('u64 16777215', BcsBuilder.u64(), 16777215n, 'ffffff0000000000');
		testType('u64 16777216', BcsBuilder.u64(), 16777216n, '0000000100000000');
		testType('u64 4294967295', BcsBuilder.u64(), 4294967295n, 'ffffffff00000000');
		testType('u64 4294967296', BcsBuilder.u64(), 4294967296n, '0000000001000000');
		testType('u64 1099511627775', BcsBuilder.u64(), 1099511627775n, 'ffffffffff000000');
		testType('u64 1099511627776', BcsBuilder.u64(), 1099511627776n, '0000000000010000');
		testType('u64 281474976710655', BcsBuilder.u64(), 281474976710655n, 'ffffffffffff0000');
		testType('u64 281474976710656', BcsBuilder.u64(), 281474976710656n, '0000000000000100');
		testType('u64 72057594037927935', BcsBuilder.u64(), 72057594037927935n, 'ffffffffffffff00');
		testType('u64 72057594037927936', BcsBuilder.u64(), 72057594037927936n, '0000000000000001');
		testType(
			'u64 18446744073709551615',
			BcsBuilder.u64(),
			18446744073709551615n,
			'ffffffffffffffff',
		);
		testType('u128 0', BcsBuilder.u128(), 0n, '00000000000000000000000000000000');
		testType('u128 1', BcsBuilder.u128(), 1n, '01000000000000000000000000000000');
		testType('u128 255', BcsBuilder.u128(), 255n, 'ff000000000000000000000000000000');
		testType(
			'u128 18446744073709551615',
			BcsBuilder.u128(),
			18446744073709551615n,
			'ffffffffffffffff0000000000000000',
		);
		testType(
			'u128 18446744073709551615',
			BcsBuilder.u128(),
			18446744073709551616n,
			'00000000000000000100000000000000',
		);
		testType(
			'u128 340282366920938463463374607431768211455',
			BcsBuilder.u128(),
			340282366920938463463374607431768211455n,
			'ffffffffffffffffffffffffffffffff',
		);
	});

	describe('vector', () => {
		testType('vector([])', BcsBuilder.vector(BcsBuilder.u8()), [], '00');
		testType('vector([1, 2, 3])', BcsBuilder.vector(BcsBuilder.u8()), [1, 2, 3], '03010203');
		testType(
			'vector([1, null, 3])',
			BcsBuilder.vector(BcsBuilder.option(BcsBuilder.u8())),
			[1, null, 3],
			// eslint-disable-next-line no-useless-concat
			'03' + '0101' + '00' + '0103',
		);
	});

	describe('fixedVector', () => {
		testType('fixedVector([])', BcsBuilder.fixedVector(0, BcsBuilder.u8()), [], '');
		testType('vector([1, 2, 3])', BcsBuilder.fixedVector(3, BcsBuilder.u8()), [1, 2, 3], '010203');
		testType(
			'fixedVector([1, null, 3])',
			BcsBuilder.fixedVector(3, BcsBuilder.option(BcsBuilder.u8())),
			[1, null, 3],
			// eslint-disable-next-line no-useless-concat
			'0101' + '00' + '0103',
		);
	});

	describe('options', () => {
		testType('optional u8 undefined', BcsBuilder.option(BcsBuilder.u8()), undefined, '00', null);
		testType('optional u8 null', BcsBuilder.option(BcsBuilder.u8()), null, '00');
		testType('optional u8 0', BcsBuilder.option(BcsBuilder.u8()), 0, '0100');
		testType(
			'optional vector(null)',
			BcsBuilder.option(BcsBuilder.vector(BcsBuilder.u8())),
			null,
			'00',
		);
		testType(
			'optional vector([1, 2, 3])',
			BcsBuilder.option(BcsBuilder.vector(BcsBuilder.option(BcsBuilder.u8()))),
			[1, null, 3],
			// eslint-disable-next-line no-useless-concat
			'01' + '03' + '0101' + '00' + '0103',
		);
	});

	describe('string', () => {
		testType('string empty', BcsBuilder.string(), '', '00');
		testType('string hello', BcsBuilder.string(), 'hello', '0568656c6c6f');
		testType(
			'string çå∞≠¢õß∂ƒ∫',
			BcsBuilder.string(),
			'çå∞≠¢õß∂ƒ∫',
			'18c3a7c3a5e2889ee289a0c2a2c3b5c39fe28882c692e288ab',
		);
	});

	describe('bytes', () => {
		testType('bytes', BcsBuilder.bytes(4), new Uint8Array([1, 2, 3, 4]), '01020304');
	});

	describe('hex', () => {
		testType('hex', BcsBuilder.hex(), '01020304', '0401020304');
	});

	describe('base64', () => {
		testType('base64', BcsBuilder.base64(), 'AQIDBA==', '0401020304');
	});

	describe('b58', () => {
		testType('b58', BcsBuilder.base58(), toB58(new Uint8Array([1, 2, 3, 4])), '0401020304');
	});

	describe('tuples', () => {
		testType('tuple(u8, u8)', BcsBuilder.tuple([BcsBuilder.u8(), BcsBuilder.u8()]), [1, 2], '0102');
		testType(
			'tuple(u8, string, boolean)',
			BcsBuilder.tuple([BcsBuilder.u8(), BcsBuilder.string(), BcsBuilder.bool()]),
			[1, 'hello', true],
			'010568656c6c6f01',
		);

		testType(
			'tuple(null, u8)',
			BcsBuilder.tuple([BcsBuilder.option(BcsBuilder.u8()), BcsBuilder.option(BcsBuilder.u8())]),
			[null, 1],
			'000101',
		);
	});

	describe('structs', () => {
		const MyStruct = BcsBuilder.struct({
			boolean: BcsBuilder.bool(),
			bytes: BcsBuilder.vector(BcsBuilder.u8()),
			label: BcsBuilder.string(),
		});

		const Wrapper = BcsBuilder.struct({
			inner: MyStruct,
			name: BcsBuilder.string(),
		});
		testType(
			'struct { boolean: bool, bytes: Vec<u8>, label: String }',
			MyStruct,
			{
				boolean: true,
				bytes: new Uint8Array([0xc0, 0xde]),
				label: 'a',
			},
			'0102c0de0161',
			{
				boolean: true,
				bytes: [0xc0, 0xde],
				label: 'a',
			},
		);

		testType(
			'struct { inner: MyStruct, name: String }',
			Wrapper,
			{
				inner: {
					boolean: true,
					bytes: new Uint8Array([0xc0, 0xde]),
					label: 'a',
				},

				name: 'b',
			},
			'0102c0de01610162',
			{
				inner: {
					boolean: true,
					bytes: [0xc0, 0xde],
					label: 'a',
				},
				name: 'b',
			},
		);
	});

	describe('enums', () => {
		const E = BcsBuilder.enum({
			Variant0: BcsBuilder.u16(),
			Variant1: BcsBuilder.u8(),
			Variant2: BcsBuilder.string(),
		});

		testType('Enum::Variant0(1)', E, { Variant0: 1 }, '000100');
		testType('Enum::Variant1(1)', E, { Variant1: 1 }, '0101');
		testType('Enum::Variant2("hello")', E, { Variant2: 'hello' }, '020568656c6c6f');
	});
});

function testType<T, Input>(
	name: string,
	schema: BcsType<T, Input>,
	value: Input,
	hex: string,
	expected: T = value as never,
) {
	test(name, () => {
		const bytes = schema.serialize(value);
		expect(toHEX(bytes)).toBe(hex);
		const deserialized = schema.parse(bytes);
		expect(deserialized).toEqual(expected);

		const writer = new BcsWriter({ size: bytes.length });
		schema.write(value, writer);
		expect(toHEX(writer.toBytes())).toBe(hex);

		const reader = new BcsReader(bytes);
		expect(schema.read(reader)).toEqual(expected);
	});
}
