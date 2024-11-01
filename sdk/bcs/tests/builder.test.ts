// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, test } from 'vitest';

import { BcsReader, BcsWriter, toBase58, toBase64, toHex } from '../src';
import { BcsType } from '../src/bcs-type.js';
import { bcs } from '../src/bcs.js';

describe('bcs', () => {
	describe('base types', () => {
		testType('true', bcs.bool(), true, '01');
		testType('false', bcs.bool(), false, '00');
		testType('uleb128 0', bcs.uleb128(), 0, '00');
		testType('uleb128 1', bcs.uleb128(), 1, '01');
		testType('uleb128 127', bcs.uleb128(), 127, '7f');
		testType('uleb128 128', bcs.uleb128(), 128, '8001');
		testType('uleb128 255', bcs.uleb128(), 255, 'ff01');
		testType('uleb128 256', bcs.uleb128(), 256, '8002');
		testType('uleb128 16383', bcs.uleb128(), 16383, 'ff7f');
		testType('uleb128 16384', bcs.uleb128(), 16384, '808001');
		testType('uleb128 2097151', bcs.uleb128(), 2097151, 'ffff7f');
		testType('uleb128 2097152', bcs.uleb128(), 2097152, '80808001');
		testType('uleb128 268435455', bcs.uleb128(), 268435455, 'ffffff7f');
		testType('uleb128 268435456', bcs.uleb128(), 268435456, '8080808001');
		testType('u8 0', bcs.u8(), 0, '00');
		testType('u8 1', bcs.u8(), 1, '01');
		testType('u8 255', bcs.u8(), 255, 'ff');
		testType('u16 0', bcs.u16(), 0, '0000');
		testType('u16 1', bcs.u16(), 1, '0100');
		testType('u16 255', bcs.u16(), 255, 'ff00');
		testType('u16 256', bcs.u16(), 256, '0001');
		testType('u16 65535', bcs.u16(), 65535, 'ffff');
		testType('u32 0', bcs.u32(), 0, '00000000');
		testType('u32 1', bcs.u32(), 1, '01000000');
		testType('u32 255', bcs.u32(), 255, 'ff000000');
		testType('u32 256', bcs.u32(), 256, '00010000');
		testType('u32 65535', bcs.u32(), 65535, 'ffff0000');
		testType('u32 65536', bcs.u32(), 65536, '00000100');
		testType('u32 16777215', bcs.u32(), 16777215, 'ffffff00');
		testType('u32 16777216', bcs.u32(), 16777216, '00000001');
		testType('u32 4294967295', bcs.u32(), 4294967295, 'ffffffff');
		testType('u64 0', bcs.u64(), 0, '0000000000000000', '0');
		testType('u64 1', bcs.u64(), 1, '0100000000000000', '1');
		testType('u64 255', bcs.u64(), 255n, 'ff00000000000000', '255');
		testType('u64 256', bcs.u64(), 256n, '0001000000000000', '256');
		testType('u64 65535', bcs.u64(), 65535n, 'ffff000000000000', '65535');
		testType('u64 65536', bcs.u64(), 65536n, '0000010000000000', '65536');
		testType('u64 16777215', bcs.u64(), 16777215n, 'ffffff0000000000', '16777215');
		testType('u64 16777216', bcs.u64(), 16777216n, '0000000100000000', '16777216');
		testType('u64 4294967295', bcs.u64(), 4294967295n, 'ffffffff00000000', '4294967295');
		testType('u64 4294967296', bcs.u64(), 4294967296n, '0000000001000000', '4294967296');
		testType('u64 1099511627775', bcs.u64(), 1099511627775n, 'ffffffffff000000', '1099511627775');
		testType('u64 1099511627776', bcs.u64(), 1099511627776n, '0000000000010000', '1099511627776');
		testType(
			'u64 281474976710655',
			bcs.u64(),
			281474976710655n,
			'ffffffffffff0000',
			'281474976710655',
		);
		testType(
			'u64 281474976710656',
			bcs.u64(),
			281474976710656n,
			'0000000000000100',
			'281474976710656',
		);
		testType(
			'u64 72057594037927935',
			bcs.u64(),
			72057594037927935n,
			'ffffffffffffff00',
			'72057594037927935',
		);
		testType(
			'u64 72057594037927936',
			bcs.u64(),
			72057594037927936n,
			'0000000000000001',
			'72057594037927936',
		);
		testType(
			'u64 18446744073709551615',
			bcs.u64(),
			18446744073709551615n,
			'ffffffffffffffff',
			'18446744073709551615',
		);
		testType('u128 0', bcs.u128(), 0n, '00000000000000000000000000000000', '0');
		testType('u128 1', bcs.u128(), 1n, '01000000000000000000000000000000', '1');
		testType('u128 255', bcs.u128(), 255n, 'ff000000000000000000000000000000', '255');
		testType(
			'u128 18446744073709551615',
			bcs.u128(),
			18446744073709551615n,
			'ffffffffffffffff0000000000000000',
			'18446744073709551615',
		);
		testType(
			'u128 18446744073709551615',
			bcs.u128(),
			18446744073709551616n,
			'00000000000000000100000000000000',
			'18446744073709551616',
		);
		testType(
			'u128 340282366920938463463374607431768211455',
			bcs.u128(),
			340282366920938463463374607431768211455n,
			'ffffffffffffffffffffffffffffffff',
			'340282366920938463463374607431768211455',
		);
	});

	describe('vector', () => {
		testType('vector([])', bcs.vector(bcs.u8()), [], '00');
		testType('vector([1, 2, 3])', bcs.vector(bcs.u8()), [1, 2, 3], '03010203');
		testType(
			'vector([1, null, 3])',
			bcs.vector(bcs.option(bcs.u8())),
			[1, null, 3],
			// eslint-disable-next-line no-useless-concat
			'03' + '0101' + '00' + '0103',
		);
	});

	describe('fixedVector', () => {
		testType('fixedVector([])', bcs.fixedArray(0, bcs.u8()), [], '');
		testType('vector([1, 2, 3])', bcs.fixedArray(3, bcs.u8()), [1, 2, 3], '010203');
		testType(
			'fixedVector([1, null, 3])',
			bcs.fixedArray(3, bcs.option(bcs.u8())),
			[1, null, 3],
			// eslint-disable-next-line no-useless-concat
			'0101' + '00' + '0103',
		);
	});

	describe('options', () => {
		testType('optional u8 undefined', bcs.option(bcs.u8()), undefined, '00', null);
		testType('optional u8 null', bcs.option(bcs.u8()), null, '00');
		testType('optional u8 0', bcs.option(bcs.u8()), 0, '0100');
		testType('optional vector(null)', bcs.option(bcs.vector(bcs.u8())), null, '00');
		testType(
			'optional vector([1, 2, 3])',
			bcs.option(bcs.vector(bcs.option(bcs.u8()))),
			[1, null, 3],
			// eslint-disable-next-line no-useless-concat
			'01' + '03' + '0101' + '00' + '0103',
		);
	});

	describe('string', () => {
		testType('string empty', bcs.string(), '', '00');
		testType('string hello', bcs.string(), 'hello', '0568656c6c6f');
		testType(
			'string çå∞≠¢õß∂ƒ∫',
			bcs.string(),
			'çå∞≠¢õß∂ƒ∫',
			'18c3a7c3a5e2889ee289a0c2a2c3b5c39fe28882c692e288ab',
		);
	});

	describe('bytes', () => {
		testType('bytes', bcs.bytes(4), new Uint8Array([1, 2, 3, 4]), '01020304');
	});

	describe('tuples', () => {
		testType('tuple(u8, u8)', bcs.tuple([bcs.u8(), bcs.u8()]), [1, 2], '0102');
		testType(
			'tuple(u8, string, boolean)',
			bcs.tuple([bcs.u8(), bcs.string(), bcs.bool()]),
			[1, 'hello', true],
			'010568656c6c6f01',
		);

		testType(
			'tuple(null, u8)',
			bcs.tuple([bcs.option(bcs.u8()), bcs.option(bcs.u8())]),
			[null, 1],
			'000101',
		);
	});

	describe('structs', () => {
		const MyStruct = bcs.struct('MyStruct', {
			boolean: bcs.bool(),
			bytes: bcs.vector(bcs.u8()),
			label: bcs.string(),
		});

		const Wrapper = bcs.struct('Wrapper', {
			inner: MyStruct,
			name: bcs.string(),
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
		const E = bcs.enum('E', {
			Variant0: bcs.u16(),
			Variant1: bcs.u8(),
			Variant2: bcs.string(),
		});

		testType('Enum::Variant0(1)', E, { Variant0: 1 }, '000100', { $kind: 'Variant0', Variant0: 1 });
		testType('Enum::Variant1(1)', E, { Variant1: 1 }, '0101', { $kind: 'Variant1', Variant1: 1 });
		testType('Enum::Variant2("hello")', E, { Variant2: 'hello' }, '020568656c6c6f', {
			$kind: 'Variant2',
			Variant2: 'hello',
		});
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
		const serialized = schema.serialize(value);
		const bytes = serialized.toBytes();
		expect(toHex(bytes)).toBe(hex);
		expect(serialized.toHex()).toBe(hex);
		expect(serialized.toBase64()).toBe(toBase64(bytes));
		expect(serialized.toBase58()).toBe(toBase58(bytes));

		const deserialized = schema.parse(bytes);
		expect(deserialized).toEqual(expected);

		const writer = new BcsWriter({ initialSize: bytes.length });
		schema.write(value, writer);
		expect(toHex(writer.toBytes())).toBe(hex);

		const reader = new BcsReader(bytes);

		expect(schema.read(reader)).toEqual(expected);
	});
}
