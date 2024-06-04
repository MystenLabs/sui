// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { BcsTypeOptions } from './bcs-type.js';
import {
	BcsType,
	bigUIntBcsType,
	dynamicSizeBcsType,
	fixedSizeBcsType,
	lazyBcsType,
	stringLikeBcsType,
	uIntBcsType,
} from './bcs-type.js';
import type { EnumInputShape, EnumOutputShape } from './types.js';
import { ulebEncode } from './uleb.js';

export const bcs = {
	/**
	 * Creates a BcsType that can be used to read and write an 8-bit unsigned integer.
	 * @example
	 * bcs.u8().serialize(255).toBytes() // Uint8Array [ 255 ]
	 */
	u8(options?: BcsTypeOptions<number>) {
		return uIntBcsType({
			name: 'u8',
			readMethod: 'read8',
			writeMethod: 'write8',
			size: 1,
			maxValue: 2 ** 8 - 1,
			...options,
		});
	},

	/**
	 * Creates a BcsType that can be used to read and write a 16-bit unsigned integer.
	 * @example
	 * bcs.u16().serialize(65535).toBytes() // Uint8Array [ 255, 255 ]
	 */
	u16(options?: BcsTypeOptions<number>) {
		return uIntBcsType({
			name: 'u16',
			readMethod: 'read16',
			writeMethod: 'write16',
			size: 2,
			maxValue: 2 ** 16 - 1,
			...options,
		});
	},

	/**
	 * Creates a BcsType that can be used to read and write a 32-bit unsigned integer.
	 * @example
	 * bcs.u32().serialize(4294967295).toBytes() // Uint8Array [ 255, 255, 255, 255 ]
	 */
	u32(options?: BcsTypeOptions<number>) {
		return uIntBcsType({
			name: 'u32',
			readMethod: 'read32',
			writeMethod: 'write32',
			size: 4,
			maxValue: 2 ** 32 - 1,
			...options,
		});
	},

	/**
	 * Creates a BcsType that can be used to read and write a 64-bit unsigned integer.
	 * @example
	 * bcs.u64().serialize(1).toBytes() // Uint8Array [ 1, 0, 0, 0, 0, 0, 0, 0 ]
	 */
	u64(options?: BcsTypeOptions<string, number | bigint | string>) {
		return bigUIntBcsType({
			name: 'u64',
			readMethod: 'read64',
			writeMethod: 'write64',
			size: 8,
			maxValue: 2n ** 64n - 1n,
			...options,
		});
	},

	/**
	 * Creates a BcsType that can be used to read and write a 128-bit unsigned integer.
	 * @example
	 * bcs.u128().serialize(1).toBytes() // Uint8Array [ 1, ..., 0 ]
	 */
	u128(options?: BcsTypeOptions<string, number | bigint | string>) {
		return bigUIntBcsType({
			name: 'u128',
			readMethod: 'read128',
			writeMethod: 'write128',
			size: 16,
			maxValue: 2n ** 128n - 1n,
			...options,
		});
	},

	/**
	 * Creates a BcsType that can be used to read and write a 256-bit unsigned integer.
	 * @example
	 * bcs.u256().serialize(1).toBytes() // Uint8Array [ 1, ..., 0 ]
	 */
	u256(options?: BcsTypeOptions<string, number | bigint | string>) {
		return bigUIntBcsType({
			name: 'u256',
			readMethod: 'read256',
			writeMethod: 'write256',
			size: 32,
			maxValue: 2n ** 256n - 1n,
			...options,
		});
	},

	/**
	 * Creates a BcsType that can be used to read and write boolean values.
	 * @example
	 * bcs.bool().serialize(true).toBytes() // Uint8Array [ 1 ]
	 */
	bool(options?: BcsTypeOptions<boolean>) {
		return fixedSizeBcsType<boolean>({
			name: 'bool',
			size: 1,
			read: (reader) => reader.read8() === 1,
			write: (value, writer) => writer.write8(value ? 1 : 0),
			...options,
			validate: (value) => {
				options?.validate?.(value);
				if (typeof value !== 'boolean') {
					throw new TypeError(`Expected boolean, found ${typeof value}`);
				}
			},
		});
	},

	/**
	 * Creates a BcsType that can be used to read and write unsigned LEB encoded integers
	 * @example
	 *
	 */
	uleb128(options?: BcsTypeOptions<number>) {
		return dynamicSizeBcsType<number>({
			name: 'uleb128',
			read: (reader) => reader.readULEB(),
			serialize: (value) => {
				return Uint8Array.from(ulebEncode(value));
			},
			...options,
		});
	},

	/**
	 * Creates a BcsType representing a fixed length byte array
	 * @param size The number of bytes this types represents
	 * @example
	 * bcs.bytes(3).serialize(new Uint8Array([1, 2, 3])).toBytes() // Uint8Array [1, 2, 3]
	 */
	bytes<T extends number>(size: T, options?: BcsTypeOptions<Uint8Array, Iterable<number>>) {
		return fixedSizeBcsType<Uint8Array>({
			name: `bytes[${size}]`,
			size,
			read: (reader) => reader.readBytes(size),
			write: (value, writer) => {
				for (let i = 0; i < size; i++) {
					writer.write8(value[i] ?? 0);
				}
			},
			...options,
			validate: (value) => {
				options?.validate?.(value);
				if (!value || typeof value !== 'object' || !('length' in value)) {
					throw new TypeError(`Expected array, found ${typeof value}`);
				}
				if (value.length !== size) {
					throw new TypeError(`Expected array of length ${size}, found ${value.length}`);
				}
			},
		});
	},

	/**
	 * Creates a BcsType that can ser/de string values.  Strings will be UTF-8 encoded
	 * @example
	 * bcs.string().serialize('a').toBytes() // Uint8Array [ 1, 97 ]
	 */
	string(options?: BcsTypeOptions<string>) {
		return stringLikeBcsType({
			name: 'string',
			toBytes: (value) => new TextEncoder().encode(value),
			fromBytes: (bytes) => new TextDecoder().decode(bytes),
			...options,
		});
	},

	/**
	 * Creates a BcsType that represents a fixed length array of a given type
	 * @param size The number of elements in the array
	 * @param type The BcsType of each element in the array
	 * @example
	 * bcs.fixedArray(3, bcs.u8()).serialize([1, 2, 3]).toBytes() // Uint8Array [ 1, 2, 3 ]
	 */
	fixedArray<T, Input>(
		size: number,
		type: BcsType<T, Input>,
		options?: BcsTypeOptions<T[], Iterable<Input> & { length: number }>,
	) {
		return new BcsType<T[], Iterable<Input> & { length: number }>({
			name: `${type.name}[${size}]`,
			read: (reader) => {
				const result: T[] = new Array(size);
				for (let i = 0; i < size; i++) {
					result[i] = type.read(reader);
				}
				return result;
			},
			write: (value, writer) => {
				for (const item of value) {
					type.write(item, writer);
				}
			},
			...options,
			validate: (value) => {
				options?.validate?.(value);
				if (!value || typeof value !== 'object' || !('length' in value)) {
					throw new TypeError(`Expected array, found ${typeof value}`);
				}
				if (value.length !== size) {
					throw new TypeError(`Expected array of length ${size}, found ${value.length}`);
				}
			},
		});
	},

	/**
	 * Creates a BcsType representing an optional value
	 * @param type The BcsType of the optional value
	 * @example
	 * bcs.option(bcs.u8()).serialize(null).toBytes() // Uint8Array [ 0 ]
	 * bcs.option(bcs.u8()).serialize(1).toBytes() // Uint8Array [ 1, 1 ]
	 */
	option<T, Input>(type: BcsType<T, Input>) {
		return bcs
			.enum(`Option<${type.name}>`, {
				None: null,
				Some: type,
			})
			.transform({
				input: (value: Input | null | undefined) => {
					if (value == null) {
						return { None: true };
					}

					return { Some: value };
				},
				output: (value) => {
					if (value.$kind === 'Some') {
						return value.Some;
					}

					return null;
				},
			});
	},

	/**
	 * Creates a BcsType representing a variable length vector of a given type
	 * @param type The BcsType of each element in the vector
	 *
	 * @example
	 * bcs.vector(bcs.u8()).toBytes([1, 2, 3]) // Uint8Array [ 3, 1, 2, 3 ]
	 */
	vector<T, Input>(
		type: BcsType<T, Input>,
		options?: BcsTypeOptions<T[], Iterable<Input> & { length: number }>,
	) {
		return new BcsType<T[], Iterable<Input> & { length: number }>({
			name: `vector<${type.name}>`,
			read: (reader) => {
				const length = reader.readULEB();
				const result: T[] = new Array(length);
				for (let i = 0; i < length; i++) {
					result[i] = type.read(reader);
				}
				return result;
			},
			write: (value, writer) => {
				writer.writeULEB(value.length);
				for (const item of value) {
					type.write(item, writer);
				}
			},
			...options,
			validate: (value) => {
				options?.validate?.(value);
				if (!value || typeof value !== 'object' || !('length' in value)) {
					throw new TypeError(`Expected array, found ${typeof value}`);
				}
			},
		});
	},

	/**
	 * Creates a BcsType representing a tuple of a given set of types
	 * @param types The BcsTypes for each element in the tuple
	 *
	 * @example
	 * const tuple = bcs.tuple([bcs.u8(), bcs.string(), bcs.bool()])
	 * tuple.serialize([1, 'a', true]).toBytes() // Uint8Array [ 1, 1, 97, 1 ]
	 */
	tuple<const Types extends readonly BcsType<any>[]>(
		types: Types,
		options?: BcsTypeOptions<
			{
				-readonly [K in keyof Types]: Types[K] extends BcsType<infer T, any> ? T : never;
			},
			{
				[K in keyof Types]: Types[K] extends BcsType<any, infer T> ? T : never;
			}
		>,
	) {
		return new BcsType<
			{
				-readonly [K in keyof Types]: Types[K] extends BcsType<infer T, any> ? T : never;
			},
			{
				[K in keyof Types]: Types[K] extends BcsType<any, infer T> ? T : never;
			}
		>({
			name: `(${types.map((t) => t.name).join(', ')})`,
			serializedSize: (values) => {
				let total = 0;
				for (let i = 0; i < types.length; i++) {
					const size = types[i].serializedSize(values[i]);
					if (size == null) {
						return null;
					}

					total += size;
				}

				return total;
			},
			read: (reader) => {
				const result: unknown[] = [];
				for (const type of types) {
					result.push(type.read(reader));
				}
				return result as never;
			},
			write: (value, writer) => {
				for (let i = 0; i < types.length; i++) {
					types[i].write(value[i], writer);
				}
			},
			...options,
			validate: (value) => {
				options?.validate?.(value);
				if (!Array.isArray(value)) {
					throw new TypeError(`Expected array, found ${typeof value}`);
				}
				if (value.length !== types.length) {
					throw new TypeError(`Expected array of length ${types.length}, found ${value.length}`);
				}
			},
		});
	},

	/**
	 * Creates a BcsType representing a struct of a given set of fields
	 * @param name The name of the struct
	 * @param fields The fields of the struct. The order of the fields affects how data is serialized and deserialized
	 *
	 * @example
	 * const struct = bcs.struct('MyStruct', {
	 *  a: bcs.u8(),
	 *  b: bcs.string(),
	 * })
	 * struct.serialize({ a: 1, b: 'a' }).toBytes() // Uint8Array [ 1, 1, 97 ]
	 */
	struct<T extends Record<string, BcsType<any>>>(
		name: string,
		fields: T,
		options?: Omit<
			BcsTypeOptions<
				{
					[K in keyof T]: T[K] extends BcsType<infer U, any> ? U : never;
				},
				{
					[K in keyof T]: T[K] extends BcsType<any, infer U> ? U : never;
				}
			>,
			'name'
		>,
	) {
		const canonicalOrder = Object.entries(fields);

		return new BcsType<
			{
				[K in keyof T]: T[K] extends BcsType<infer U, any> ? U : never;
			},
			{
				[K in keyof T]: T[K] extends BcsType<any, infer U> ? U : never;
			}
		>({
			name,
			serializedSize: (values) => {
				let total = 0;
				for (const [field, type] of canonicalOrder) {
					const size = type.serializedSize(values[field]);
					if (size == null) {
						return null;
					}

					total += size;
				}

				return total;
			},
			read: (reader) => {
				const result: Record<string, unknown> = {};
				for (const [field, type] of canonicalOrder) {
					result[field] = type.read(reader);
				}

				return result as never;
			},
			write: (value, writer) => {
				for (const [field, type] of canonicalOrder) {
					type.write(value[field], writer);
				}
			},
			...options,
			validate: (value) => {
				options?.validate?.(value);
				if (typeof value !== 'object' || value == null) {
					throw new TypeError(`Expected object, found ${typeof value}`);
				}
			},
		});
	},

	/**
	 * Creates a BcsType representing an enum of a given set of options
	 * @param name The name of the enum
	 * @param values The values of the enum. The order of the values affects how data is serialized and deserialized.
	 * null can be used to represent a variant with no data.
	 *
	 * @example
	 * const enum = bcs.enum('MyEnum', {
	 *   A: bcs.u8(),
	 *   B: bcs.string(),
	 *   C: null,
	 * })
	 * enum.serialize({ A: 1 }).toBytes() // Uint8Array [ 0, 1 ]
	 * enum.serialize({ B: 'a' }).toBytes() // Uint8Array [ 1, 1, 97 ]
	 * enum.serialize({ C: true }).toBytes() // Uint8Array [ 2 ]
	 */
	enum<T extends Record<string, BcsType<any> | null>>(
		name: string,
		values: T,
		options?: Omit<
			BcsTypeOptions<
				EnumOutputShape<{
					[K in keyof T]: T[K] extends BcsType<infer U, any> ? U : true;
				}>,
				EnumInputShape<{
					[K in keyof T]: T[K] extends BcsType<any, infer U> ? U : boolean | object | null;
				}>
			>,
			'name'
		>,
	) {
		const canonicalOrder = Object.entries(values as object);
		return new BcsType<
			EnumOutputShape<{
				[K in keyof T]: T[K] extends BcsType<infer U, any> ? U : true;
			}>,
			EnumInputShape<{
				[K in keyof T]: T[K] extends BcsType<any, infer U> ? U : boolean | object | null;
			}>
		>({
			name,
			read: (reader) => {
				const index = reader.readULEB();
				const [name, type] = canonicalOrder[index];
				return {
					[name]: type?.read(reader) ?? true,
					$kind: name,
				} as never;
			},
			write: (value, writer) => {
				const [name, val] = Object.entries(value).filter(([name]) =>
					Object.hasOwn(values, name),
				)[0];

				for (let i = 0; i < canonicalOrder.length; i++) {
					const [optionName, optionType] = canonicalOrder[i];
					if (optionName === name) {
						writer.writeULEB(i);
						optionType?.write(val, writer);
						return;
					}
				}
			},
			...options,
			validate: (value) => {
				options?.validate?.(value);
				if (typeof value !== 'object' || value == null) {
					throw new TypeError(`Expected object, found ${typeof value}`);
				}

				const keys = Object.keys(value).filter(
					(k) => value[k] !== undefined && Object.hasOwn(values, k),
				);

				if (keys.length !== 1) {
					throw new TypeError(
						`Expected object with one key, but found ${keys.length} for type ${name}}`,
					);
				}

				const [variant] = keys;

				if (!Object.hasOwn(values, variant)) {
					throw new TypeError(`Invalid enum variant ${variant}`);
				}
			},
		});
	},

	/**
	 * Creates a BcsType representing a map of a given key and value type
	 * @param keyType The BcsType of the key
	 * @param valueType The BcsType of the value
	 * @example
	 * const map = bcs.map(bcs.u8(), bcs.string())
	 * map.serialize(new Map([[2, 'a']])).toBytes() // Uint8Array [ 1, 2, 1, 97 ]
	 */
	map<K, V, InputK = K, InputV = V>(keyType: BcsType<K, InputK>, valueType: BcsType<V, InputV>) {
		return bcs.vector(bcs.tuple([keyType, valueType])).transform({
			name: `Map<${keyType.name}, ${valueType.name}>`,
			input: (value: Map<InputK, InputV>) => {
				return [...value.entries()];
			},
			output: (value) => {
				const result = new Map<K, V>();
				for (const [key, val] of value) {
					result.set(key, val);
				}
				return result;
			},
		});
	},

	/**
	 * Creates a BcsType that wraps another BcsType which is lazily evaluated. This is useful for creating recursive types.
	 * @param cb A callback that returns the BcsType
	 */
	lazy<T extends BcsType<any>>(cb: () => T): T {
		return lazyBcsType(cb) as T;
	},
};
