// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Please, use the code from this file to fill in the examples in
 * the README. Manual needs to be correct for the best DevX.
 */

import { describe, it } from 'vitest';
import { SUI_ADDRESS_LENGTH } from '../../typescript/src';
import { BCS, BcsWriter, getRustConfig, getSuiMoveConfig } from './../src/index';

describe('BCS: README Examples', () => {
	it('quick start', () => {
		const bcs = new BCS(getSuiMoveConfig());

		// registering types
		bcs.registerAlias('UID', BCS.ADDRESS);
		bcs.registerEnumType('Option<T>', {
			none: null,
			some: 'T',
		});
		bcs.registerStructType('Coin', {
			id: 'UID',
			value: BCS.U64,
		});

		// deserialization: BCS bytes into Coin
		let bytes = bcs
			.ser('Coin', {
				id: '0000000000000000000000000000000000000000000000000000000000000001',
				value: 1000000n,
			})
			.toBytes();

		let coin = bcs.de('Coin', bytes);

		// serialization: Object into bytes
		let _data = bcs.ser('Option<Coin>', { some: coin }).toString('hex');
	});

	it('Example: All options used', () => {
		const bcs = new BCS({
			vectorType: 'vector<T>',
			addressLength: SUI_ADDRESS_LENGTH,
			addressEncoding: 'hex',
			genericSeparators: ['<', '>'],
			types: {
				// define schema in the initializer
				structs: {
					User: {
						name: BCS.STRING,
						age: BCS.U8,
					},
				},
				enums: {},
				aliases: { hex: BCS.HEX },
			},
			withPrimitives: true,
		});

		let _bytes = bcs.ser('User', { name: 'Adam', age: '30' }).toString('base64');
	});

	it('initialization', () => {
		const bcs = new BCS(getSuiMoveConfig());

		// use bcs.ser() to serialize data
		const val = [1, 2, 3, 4];
		const ser = bcs.ser('vector<u8>', val).toBytes();

		// use bcs.de() to deserialize data
		const res = bcs.de('vector<u8>', ser);

		console.assert(res.toString() === val.toString());
	});

	it('Example: Rust Config', () => {
		const bcs = new BCS(getRustConfig());
		const val = [1, 2, 3, 4];
		const ser = bcs.ser('Vec<u8>', val).toBytes();
		const res = bcs.de('Vec<u8>', ser);

		console.assert(res.toString() === val.toString());
	});

	it('Example: Primitive types', () => {
		const bcs = new BCS(getSuiMoveConfig());

		// Integers
		let _u8 = bcs.ser(BCS.U8, 100).toBytes();
		let _u64 = bcs.ser(BCS.U64, 1000000n).toString('hex');
		let _u128 = bcs.ser(BCS.U128, '100000010000001000000').toString('base64');

		// Other types
		let _bool = bcs.ser(BCS.BOOL, true).toString('hex');
		let _addr = bcs.ser(BCS.ADDRESS, '0000000000000000000000000000000000000001').toBytes();
		let _str = bcs.ser(BCS.STRING, 'this is an ascii string').toBytes();

		// Vectors (vector<T>)
		let _u8_vec = bcs.ser('vector<u8>', [1, 2, 3, 4, 5, 6, 7]).toBytes();
		let _bool_vec = bcs.ser('vector<bool>', [true, true, false]).toBytes();
		let _str_vec = bcs.ser('vector<bool>', ['string1', 'string2', 'string3']).toBytes();

		// Even vector of vector (...of vector) is an option
		let _matrix = bcs
			.ser('vector<vector<u8>>', [
				[0, 0, 0],
				[1, 1, 1],
				[2, 2, 2],
			])
			.toBytes();
	});

	it('Example: Ser/de and Encoding', () => {
		const bcs = new BCS(getSuiMoveConfig());

		// bcs.ser() returns an instance of BcsWriter which can be converted to bytes or a string
		let bcsWriter: BcsWriter = bcs.ser(BCS.STRING, 'this is a string');

		// writer.toBytes() returns a Uint8Array
		let bytes: Uint8Array = bcsWriter.toBytes();

		// custom encodings can be chosen when needed (just like Buffer)
		let hex: string = bcsWriter.toString('hex');
		let base64: string = bcsWriter.toString('base64');
		let base58: string = bcsWriter.toString('base58');

		// bcs.de() reads BCS data and returns the value
		// by default it expects data to be `Uint8Array`
		let str1 = bcs.de(BCS.STRING, bytes);

		// alternatively, an encoding of input can be specified
		let str2 = bcs.de(BCS.STRING, hex, 'hex');
		let str3 = bcs.de(BCS.STRING, base64, 'base64');
		let str4 = bcs.de(BCS.STRING, base58, 'base58');

		console.assert((str1 === str2) === (str3 === str4), 'Result is the same');
	});

	it('Example: Alias', () => {
		const bcs = new BCS(getSuiMoveConfig());

		// When registering alias simply specify a new name for the type
		bcs.registerAlias('ObjectDigest', BCS.BASE58);

		// ObjectDigest is now treated as base58 string
		let _b58 = bcs.ser('ObjectDigest', 'Ldp').toBytes();

		// we can override already existing definition
		bcs.registerAlias('ObjectDigest', BCS.HEX);

		let _hex = bcs.ser('ObjectDigest', 'C0FFEE').toBytes();
	});

	it('Example: Struct', () => {
		const bcs = new BCS(getSuiMoveConfig());

		// register a custom type (it becomes available for using)
		bcs.registerStructType('Balance', {
			value: BCS.U64,
		});

		bcs.registerStructType('Coin', {
			id: BCS.ADDRESS,
			// reference another registered type
			balance: 'Balance',
		});

		// value passed into ser function has to have the same
		// structure as the definition
		let _bytes = bcs
			.ser('Coin', {
				id: '0x0000000000000000000000000000000000000000000000000000000000000005',
				balance: {
					value: 100000000n,
				},
			})
			.toBytes();
	});

	it('Example: Generics', () => {
		const bcs = new BCS(getSuiMoveConfig());

		// Container -> the name of the type
		// T -> type parameter which has to be passed in `ser()` or `de()` methods
		// If you're not familiar with generics, treat them as type Templates
		bcs.registerStructType(['Container', 'T'], {
			contents: 'T',
		});

		// When serializing, we have to pass the type to use for `T`
		bcs
			.ser(['Container', BCS.U8], {
				contents: 100,
			})
			.toString('hex');

		// Reusing the same Container type with different contents.
		// Mind that generics need to be passed as Array after the main type.
		bcs
			.ser(['Container', ['vector', BCS.BOOL]], {
				contents: [true, false, true],
			})
			.toString('hex');

		// Using multiple generics - you can use any string for convenience and
		// readability. See how we also use array notation for a field definition.
		bcs.registerStructType(['VecMap', 'Key', 'Val'], {
			keys: ['vector', 'Key'],
			values: ['vector', 'Val'],
		});

		// To serialize VecMap, we can use:
		bcs.ser(['VecMap', BCS.STRING, BCS.STRING], {
			keys: ['key1', 'key2', 'key3'],
			values: ['value1', 'value2', 'value3'],
		});
	});

	it('Example: Enum', () => {
		const bcs = new BCS(getSuiMoveConfig());

		bcs.registerEnumType('Option<T>', {
			none: null,
			some: 'T',
		});

		bcs.registerEnumType('TransactionType', {
			single: 'vector<u8>',
			batch: 'vector<vector<u8>>',
		});

		// any truthy value marks empty in struct value
		let _optionNone = bcs.ser('Option<TransactionType>', {
			none: true,
		});

		// some now contains a value of type TransactionType
		let _optionTx = bcs.ser('Option<TransactionType>', {
			some: {
				single: [1, 2, 3, 4, 5, 6],
			},
		});

		// same type signature but a different enum invariant - batch
		let _optionTxBatch = bcs.ser('Option<TransactionType>', {
			some: {
				batch: [
					[1, 2, 3, 4, 5, 6],
					[1, 2, 3, 4, 5, 6],
				],
			},
		});
	});

	it('Example: Inline Struct', () => {
		const bcs = new BCS(getSuiMoveConfig());

		// Some value we want to serialize
		const coin = {
			id: '0000000000000000000000000000000000000000000000000000000000000005',
			value: 1111333333222n,
		};

		// Instead of defining a type we pass struct schema as the first argument
		let coin_bytes = bcs.ser({ id: BCS.ADDRESS, value: BCS.U64 }, coin).toBytes();

		// Same with deserialization
		let coin_restored = bcs.de({ id: BCS.ADDRESS, value: BCS.U64 }, coin_bytes);

		console.assert(coin.id === coin_restored.id, '`id` must match');
		console.assert(coin.value === coin_restored.value, '`value` must match');
	});
});
