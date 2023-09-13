// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { BcsReader, BcsWriter } from './index.js';
import { toHEX, fromHEX } from './hex.js';
import { fromB64, toB64 } from './b64.js';
import { fromB58, toB58 } from './b58.js';
import { ulebEncode } from './uleb.js';

export const BcsBuilder = {
	u8() {
		return new UIntBcsType({
			readMethod: 'read8',
			writeMethod: 'write8',
			size: 1,
		});
	},

	u16() {
		return new UIntBcsType({
			readMethod: 'read16',
			writeMethod: 'write16',
			size: 2,
		});
	},

	u32() {
		return new UIntBcsType({
			readMethod: 'read32',
			writeMethod: 'write32',
			size: 4,
		});
	},

	u64() {
		return new BigUIntBcsType({
			readMethod: 'read64',
			writeMethod: 'write64',
			size: 8,
		});
	},

	u128() {
		return new BigUIntBcsType({
			readMethod: 'read128',
			writeMethod: 'write128',
			size: 16,
		});
	},

	u256() {
		return new BigUIntBcsType({
			readMethod: 'read256',
			writeMethod: 'write256',
			size: 32,
		});
	},

	bool() {
		return new FixedSizeBcsType<boolean>({
			size: 1,
			read: (reader) => reader.read8() === 1,
			write: (value, writer) => writer.write8(value ? 1 : 0),
		});
	},

	// TODO should be a bigint
	uleb128() {
		return new DynamicSizeBcsType<number>({
			read: (reader) => reader.readULEB(),
			serialize: (value) => {
				return Uint8Array.from(ulebEncode(value));
			},
		});
	},

	bytes(size: number) {
		return new FixedSizeBcsType<Uint8Array>({
			size,
			read: (reader) => reader.readBytes(size),
			write: (value, writer) => {
				for (let i = 0; i < size; i++) {
					writer.write8(value[i] ?? 0);
				}
			},
		});
	},

	string() {
		return new StringLikeBcsType({
			toBytes: (value) => new TextEncoder().encode(value),
			fromBytes: (bytes) => new TextDecoder().decode(bytes),
		});
	},

	hex() {
		return new StringLikeBcsType({
			toBytes: (value) => fromHEX(value),
			fromBytes: (bytes) => toHEX(bytes),
		});
	},

	base58() {
		return new StringLikeBcsType({
			toBytes: (value) => fromB58(value),
			fromBytes: (bytes) => toB58(bytes),
		});
	},

	base64() {
		return new StringLikeBcsType({
			toBytes: (value) => fromB64(value),
			fromBytes: (bytes) => toB64(bytes),
		});
	},

	option<T, Input>(type: BcsType<T, Input>) {
		return new BcsType<T | null, Input | null | undefined>({
			serializedSize: (value) => {
				if (value == null) {
					return 1;
				}

				const size = type.serializedSize(value);

				if (size == null) {
					return null;
				}

				return size + 1;
			},
			read: (reader) => {
				if (reader.read8() === 0) {
					return null;
				}
				return type.read(reader);
			},
			write: (value, writer) => {
				if (value == null) {
					writer.write8(0);
				} else {
					writer.write8(1);
					type.write(value, writer);
				}
			},
		});
	},

	vector<T, Input>(type: BcsType<T, Input>) {
		return new BcsType<T[], Iterable<Input> & { length: number }>({
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

	fixedVector<T, Input>(size: number, type: BcsType<T, Input>) {
		return new BcsType<T[], Iterable<Input> & { length: number }>({
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

	tuple<const Types extends readonly BcsType<any>[]>(types: Types) {
		return new BcsType<
			{
				-readonly [K in keyof Types]: Types[K] extends BcsType<infer T> ? T : never;
			},
			{
				[K in keyof Types]: Types[K] extends BcsType<unknown, infer T> ? T : never;
			}
		>({
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

	struct<T extends Record<string, BcsType<any>>>(fields: T) {
		const canonicalOrder = Object.entries(fields);

		return new BcsType<
			{
				[K in keyof T]: T[K] extends BcsType<infer U> ? U : never;
			},
			{
				[K in keyof T]: T[K] extends BcsType<unknown, infer U> ? U : never;
			}
		>({
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
		});
	},

	enum<T extends Record<string, BcsType<any>>>(options: T) {
		const canonicalOrder = Object.entries(options);

		return new BcsType<
			{
				[K in keyof T]: T[K] extends BcsType<infer U> ? { [k2 in keyof K]: U } : never;
			}[string],
			{
				[K in keyof T]: T[K] extends BcsType<unknown, infer U> ? { [k2 in keyof K]: U } : never;
			}[string]
		>({
			read: (reader) => {
				const index = reader.readULEB();
				const [name, type] = canonicalOrder[index];
				return {
					[name]: type.read(reader),
				} as never;
			},
			write: (value, writer) => {
				const [name, val] = Object.entries(value)[0];

				for (let i = 0; i < canonicalOrder.length; i++) {
					const [optionName, optionType] = canonicalOrder[i];
					if (optionName === name) {
						writer.writeULEB(i);
						optionType.write(val, writer);
						return;
					}
				}
			},
		});
	},

	map<K, V, InputK = K, InputV = V>(keyType: BcsType<K, InputK>, valueType: BcsType<V, InputV>) {
		return BcsBuilder.vector(BcsBuilder.tuple([keyType, valueType])).transform({
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
};

export class BcsType<T, Input = T> {
	read: (reader: BcsReader) => T;
	write: (value: Input, writer: BcsWriter) => void;
	serialize: (value: Input) => Uint8Array;
	serializedSize: (value: Input) => number | null;

	constructor(options: {
		read: (reader: BcsReader) => T;
		write: (value: Input, writer: BcsWriter) => void;
		serialize?: (value: Input) => Uint8Array;
		serializedSize?: (value: Input) => number | null;
	}) {
		this.read = options.read;
		this.write = options.write;
		this.serializedSize = options.serializedSize ?? (() => null);
		this.serialize =
			options.serialize ??
			((value) => {
				const writer = new BcsWriter({ size: this.serializedSize(value) ?? undefined });
				this.write(value, writer);
				return writer.toBytes();
			});
	}

	parse(bytes: Uint8Array): T {
		const reader = new BcsReader(bytes);
		return this.read(reader);
	}

	transform<T2, Input2>({
		input,
		output,
	}: {
		input: (val: Input2) => Input;
		output: (value: T) => T2;
	}) {
		return new BcsType<T2, Input2>({
			read: (reader) => output(this.read(reader)),
			write: (value, writer) => this.write(input(value), writer),
			serializedSize: (value) => this.serializedSize(input(value)),
			serialize: (value) => this.serialize(input(value)),
		});
	}
}

export class FixedSizeBcsType<T, Input = T> extends BcsType<T, Input> {
	size: number;

	constructor({
		size,
		...options
	}: {
		size: number;
		read: (reader: BcsReader) => T;
		write: (value: Input, writer: BcsWriter) => void;
	}) {
		super({
			...options,
			serializedSize: () => size,
		});
		this.size = size;
	}

	parse(bytes: Uint8Array): T {
		if (bytes.length !== this.size) {
			throw new Error(`Invalid length: ${bytes.length} != ${this.size}`);
		}
		return super.parse(bytes);
	}
}

export class UIntBcsType extends FixedSizeBcsType<number> {
	constructor({
		readMethod,
		writeMethod,
		...options
	}: {
		size: number;
		readMethod: `read${8 | 16 | 32}`;
		writeMethod: `write${8 | 16 | 32}`;
	}) {
		super({
			...options,
			read: (reader) => reader[readMethod](),
			write: (value, writer) => writer[writeMethod](value),
		});
	}
}

export class BigUIntBcsType extends FixedSizeBcsType<bigint, number | bigint> {
	constructor({
		readMethod,
		writeMethod,
		...options
	}: {
		size: number;
		readMethod: `read${64 | 128 | 256}`;
		writeMethod: `write${64 | 128 | 256}`;
	}) {
		super({
			...options,
			read: (reader) => BigInt(reader[readMethod]()),
			write: (value, writer) => writer[writeMethod](value),
		});
	}
}

export class DynamicSizeBcsType<T, Input = T> extends BcsType<T, Input> {
	constructor({
		serialize,
		...options
	}: {
		read: (reader: BcsReader) => T;
		serialize: (value: Input) => Uint8Array;
	}) {
		super({
			...options,
			serialize,
			write: (value, writer) => {
				for (const byte of this.serialize(value)) {
					writer.write8(byte);
				}
			},
		});
	}
}

export class StringLikeBcsType extends BcsType<string> {
	constructor({
		toBytes,
		fromBytes,
		...options
	}: {
		toBytes: (value: string) => Uint8Array;
		fromBytes: (bytes: Uint8Array) => string;
		serializedSize?: (value: string) => number | null;
	}) {
		super({
			...options,
			read: (reader) => {
				const length = reader.readULEB();
				const bytes = reader.readBytes(length);

				return fromBytes(bytes);
			},
			write: (hex, writer) => {
				const bytes = toBytes(hex);
				writer.writeULEB(bytes.length);
				for (let i = 0; i < bytes.length; i++) {
					writer.write8(bytes[i]);
				}
			},
			serialize: (value) => {
				const bytes = toBytes(value);
				const size = ulebEncode(bytes.length);
				const result = new Uint8Array(size.length + bytes.length);
				result.set(size, 0);
				result.set(bytes, size.length);

				return result;
			},
		});
	}
}
