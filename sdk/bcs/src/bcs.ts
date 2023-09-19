// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toHEX, fromHEX } from './hex.js';
import { fromB64, toB64 } from './b64.js';
import { fromB58, toB58 } from './b58.js';
import { ulebEncode } from './uleb.js';
import {
	BcsTypeOptions,
	BcsType,
	BigUIntBcsType,
	DynamicSizeBcsType,
	FixedSizeBcsType,
	StringLikeBcsType,
	UIntBcsType,
	LazyBcsType,
} from './bcs-type.js';
import { GenericPlaceholder, ReplaceBcsGenerics } from './types.js';

export const bcs = {
	u8(options?: BcsTypeOptions<number>) {
		return new UIntBcsType({
			name: 'u8',
			readMethod: 'read8',
			writeMethod: 'write8',
			size: 1,
			maxValue: 2 ** 8 - 1,
			...options,
		});
	},

	u16(options?: BcsTypeOptions<number>) {
		return new UIntBcsType({
			name: 'u16',
			readMethod: 'read16',
			writeMethod: 'write16',
			size: 2,
			maxValue: 2 ** 16 - 1,
			...options,
		});
	},

	u32(options?: BcsTypeOptions<number>) {
		return new UIntBcsType({
			name: 'u32',
			readMethod: 'read32',
			writeMethod: 'write32',
			size: 4,
			maxValue: 2 ** 32 - 1,
			...options,
		});
	},

	u64(options?: BcsTypeOptions<bigint, number | bigint>) {
		return new BigUIntBcsType({
			name: 'u64',
			readMethod: 'read64',
			writeMethod: 'write64',
			size: 8,
			maxValue: 2n ** 64n - 1n,
			...options,
		});
	},

	u128(options?: BcsTypeOptions<bigint, number | bigint>) {
		return new BigUIntBcsType({
			name: 'u128',
			readMethod: 'read128',
			writeMethod: 'write128',
			size: 16,
			maxValue: 2n ** 128n - 1n,
			...options,
		});
	},

	u256(options?: BcsTypeOptions<bigint, number | bigint>) {
		return new BigUIntBcsType({
			name: 'u256',
			readMethod: 'read256',
			writeMethod: 'write256',
			size: 32,
			maxValue: 2n ** 256n - 1n,
			...options,
		});
	},

	bool(options?: BcsTypeOptions<boolean>) {
		return new FixedSizeBcsType<boolean>({
			name: 'bool',
			size: 1,
			read: (reader) => reader.read8() === 1,
			write: (value, writer) => writer.write8(value ? 1 : 0),
			...options,
		});
	},

	// TODO should be a bigint?
	uleb128(options?: BcsTypeOptions<number>) {
		return new DynamicSizeBcsType<number>({
			name: 'uleb128',
			read: (reader) => reader.readULEB(),
			serialize: (value) => {
				return Uint8Array.from(ulebEncode(value));
			},
			...options,
		});
	},

	bytes<T extends number>(size: T, options?: BcsTypeOptions<Uint8Array, Iterable<number>>) {
		return new FixedSizeBcsType<Uint8Array>({
			name: `bytes[${size}]`,
			size,
			read: (reader) => reader.readBytes(size),
			write: (value, writer) => {
				for (let i = 0; i < size; i++) {
					writer.write8(value[i] ?? 0);
				}
			},
			...options,
		});
	},

	string(options?: BcsTypeOptions<string>) {
		return new StringLikeBcsType({
			name: 'string',
			toBytes: (value) => new TextEncoder().encode(value),
			fromBytes: (bytes) => new TextDecoder().decode(bytes),
			...options,
		});
	},

	hex(options?: BcsTypeOptions<string>) {
		return new StringLikeBcsType({
			name: 'hex',
			toBytes: (value) => fromHEX(value),
			fromBytes: (bytes) => toHEX(bytes),
			...options,
		});
	},

	base58(options?: BcsTypeOptions<string>) {
		return new StringLikeBcsType({
			name: 'base58',
			toBytes: (value) => fromB58(value),
			fromBytes: (bytes) => toB58(bytes),
			...options,
		});
	},

	base64(options?: BcsTypeOptions<string>) {
		return new StringLikeBcsType({
			name: 'base64',
			toBytes: (value) => fromB64(value),
			fromBytes: (bytes) => toB64(bytes),
			...options,
		});
	},

	array<T, Input>(size: number, type: BcsType<T, Input>) {
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
		});
	},

	option<T, Input>(type: BcsType<T, Input>) {
		return bcs.optionEnum(type).transform({
			input: (value: Input | null | undefined) => {
				if (value == null) {
					return { None: null };
				}

				return { Some: value };
			},
			output: (value) => {
				if ('Some' in value) {
					return value.Some;
				}

				return null;
			},
		});
	},

	optionEnum<T, Input>(type: BcsType<T, Input>) {
		return bcs.enum(`Option<${type.name}>`, {
			None: null,
			Some: type,
		});
	},

	vector<T, Input>(type: BcsType<T, Input>) {
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
		});
	},

	tuple<const Types extends readonly BcsType<any>[]>(types: Types) {
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
		});
	},

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
		});
	},

	enum<T>(
		name: string,
		values: T,
		options?: Omit<
			BcsTypeOptions<
				{
					[K in keyof T]: T[K] extends BcsType<infer U, any>
						? { [K2 in K]: U }
						: { [K2 in K]: true };
				}[keyof T],
				{
					[K in keyof T]: T[K] extends BcsType<any, infer U>
						? { [K2 in K]: U }
						: { [K2 in K]: null | boolean };
				}[keyof T]
			>,
			'name'
		>,
	) {
		const canonicalOrder = Object.entries(values as object);
		return new BcsType<
			{
				[K in keyof T]: T[K] extends BcsType<infer U, any> ? { [K2 in K]: U } : { [K2 in K]: true };
			}[keyof T],
			{
				[K in keyof T]: T[K] extends BcsType<any, infer U>
					? { [K2 in K]: U }
					: { [K2 in K]: null | boolean };
			}[keyof T]
		>({
			name,
			read: (reader) => {
				const index = reader.readULEB();
				const [name, type] = canonicalOrder[index];
				return {
					[name]: type?.read(reader) ?? true,
				} as never;
			},
			write: (value, writer) => {
				const [name, val] = Object.entries(value)[0];
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
		});
	},

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

	generic<const Names extends readonly string[], const Type extends BcsType<any>>(
		names: Names,
		cb: (...types: { [K in keyof Names]: BcsType<GenericPlaceholder<Names[K]>> }) => Type,
	): <T extends { [K in keyof Names]: BcsType<any> }>(
		...types: T
	) => ReplaceBcsGenerics<Type, Names, T> {
		return (...types) => {
			return cb(...types).transform({
				name: `${cb.name}<${types.map((t) => t.name).join(', ')}>`,
				input: (value) => value,
				output: (value) => value,
			}) as never;
		};
	},

	lazy<T extends BcsType<any>>(cb: () => T): T {
		return new LazyBcsType(cb) as never;
	},
};

export function builtInBcsTypes() {
	const compoundTypes = [
		'vector',
		'array',
		'tuple',
		'struct',
		'enum',
		'map',
		'option',
		'optionEnum',
		'generic',
		'bytes',
		'lazy',
	] as const;
	const types: Record<string, BcsType<any>> = {};
	const typeNames = Object.keys(bcs).filter(
		(key) => !compoundTypes.includes(key as never),
	) as Exclude<keyof typeof bcs, (typeof compoundTypes)[number]>[];

	for (const name of typeNames) {
		types[name] = bcs[name]();
	}

	return types as {
		[K in (typeof typeNames)[number]]: ReturnType<(typeof bcs)[K]>;
	};
}
