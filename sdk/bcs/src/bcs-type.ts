// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB58, toB58 } from './b58.js';
import { fromB64, toB64 } from './b64.js';
import { fromHEX, toHEX } from './hex.js';
import { BcsReader } from './reader.js';
import { ulebEncode } from './uleb.js';
import type { BcsWriterOptions } from './writer.js';
import { BcsWriter } from './writer.js';

export interface BcsTypeOptions<T, Input = T> {
	name?: string;
	validate?: (value: Input) => void;
}

export class BcsType<T, Input = T> {
	$inferType!: T;
	$inferInput!: Input;
	name: string;
	read: (reader: BcsReader) => T;
	serializedSize: (value: Input, options?: BcsWriterOptions) => number | null;
	validate: (value: Input) => void;
	#write: (value: Input, writer: BcsWriter) => void;
	#serialize: (value: Input, options?: BcsWriterOptions) => Uint8Array;

	constructor(
		options: {
			name: string;
			read: (reader: BcsReader) => T;
			write: (value: Input, writer: BcsWriter) => void;
			serialize?: (value: Input, options?: BcsWriterOptions) => Uint8Array;
			serializedSize?: (value: Input) => number | null;
			validate?: (value: Input) => void;
		} & BcsTypeOptions<T, Input>,
	) {
		this.name = options.name;
		this.read = options.read;
		this.serializedSize = options.serializedSize ?? (() => null);
		this.#write = options.write;
		this.#serialize =
			options.serialize ??
			((value, options) => {
				const writer = new BcsWriter({
					initialSize: this.serializedSize(value) ?? undefined,
					...options,
				});
				this.#write(value, writer);
				return writer.toBytes();
			});

		this.validate = options.validate ?? (() => {});
	}

	write(value: Input, writer: BcsWriter) {
		this.validate(value);
		this.#write(value, writer);
	}

	serialize(value: Input, options?: BcsWriterOptions) {
		this.validate(value);
		return new SerializedBcs(this, this.#serialize(value, options));
	}

	parse(bytes: Uint8Array): T {
		const reader = new BcsReader(bytes);
		return this.read(reader);
	}

	fromHex(hex: string) {
		return this.parse(fromHEX(hex));
	}

	fromBase58(b64: string) {
		return this.parse(fromB58(b64));
	}

	fromBase64(b64: string) {
		return this.parse(fromB64(b64));
	}

	transform<T2, Input2>({
		name,
		input,
		output,
		validate,
	}: {
		input: (val: Input2) => Input;
		output: (value: T) => T2;
	} & BcsTypeOptions<T2, Input2>) {
		return new BcsType<T2, Input2>({
			name: name ?? this.name,
			read: (reader) => output(this.read(reader)),
			write: (value, writer) => this.#write(input(value), writer),
			serializedSize: (value) => this.serializedSize(input(value)),
			serialize: (value, options) => this.#serialize(input(value), options),
			validate: (value) => {
				validate?.(value);
				this.validate(input(value));
			},
		});
	}
}

const SERIALIZED_BCS_BRAND = Symbol.for('@mysten/serialized-bcs') as never;
export function isSerializedBcs(obj: unknown): obj is SerializedBcs<unknown> {
	return !!obj && typeof obj === 'object' && (obj as any)[SERIALIZED_BCS_BRAND] === true;
}

export class SerializedBcs<T, Input = T> {
	#schema: BcsType<T, Input>;
	#bytes: Uint8Array;

	// Used to brand SerializedBcs so that they can be identified, even between multiple copies
	// of the @mysten/bcs package are installed
	get [SERIALIZED_BCS_BRAND]() {
		return true;
	}

	constructor(type: BcsType<T, Input>, schema: Uint8Array) {
		this.#schema = type;
		this.#bytes = schema;
	}

	toBytes() {
		return this.#bytes;
	}

	toHex() {
		return toHEX(this.#bytes);
	}

	toBase64() {
		return toB64(this.#bytes);
	}

	toBase58() {
		return toB58(this.#bytes);
	}

	parse() {
		return this.#schema.parse(this.#bytes);
	}
}

export function fixedSizeBcsType<T, Input = T>({
	size,
	...options
}: {
	name: string;
	size: number;
	read: (reader: BcsReader) => T;
	write: (value: Input, writer: BcsWriter) => void;
} & BcsTypeOptions<T, Input>) {
	return new BcsType<T, Input>({
		...options,
		serializedSize: () => size,
	});
}

export function uIntBcsType({
	readMethod,
	writeMethod,
	...options
}: {
	name: string;
	size: number;
	readMethod: `read${8 | 16 | 32}`;
	writeMethod: `write${8 | 16 | 32}`;
	maxValue: number;
} & BcsTypeOptions<number, number>) {
	return fixedSizeBcsType<number>({
		...options,
		read: (reader) => reader[readMethod](),
		write: (value, writer) => writer[writeMethod](value),
		validate: (value) => {
			if (value < 0 || value > options.maxValue) {
				throw new TypeError(
					`Invalid ${options.name} value: ${value}. Expected value in range 0-${options.maxValue}`,
				);
			}
			options.validate?.(value);
		},
	});
}

export function bigUIntBcsType({
	readMethod,
	writeMethod,
	...options
}: {
	name: string;
	size: number;
	readMethod: `read${64 | 128 | 256}`;
	writeMethod: `write${64 | 128 | 256}`;
	maxValue: bigint;
} & BcsTypeOptions<string, string | number | bigint>) {
	return fixedSizeBcsType<string, string | number | bigint>({
		...options,
		read: (reader) => reader[readMethod](),
		write: (value, writer) => writer[writeMethod](BigInt(value)),
		validate: (val) => {
			const value = BigInt(val);
			if (value < 0 || value > options.maxValue) {
				throw new TypeError(
					`Invalid ${options.name} value: ${value}. Expected value in range 0-${options.maxValue}`,
				);
			}
			options.validate?.(value);
		},
	});
}

export function dynamicSizeBcsType<T, Input = T>({
	serialize,
	...options
}: {
	name: string;
	read: (reader: BcsReader) => T;
	serialize: (value: Input, options?: BcsWriterOptions) => Uint8Array;
} & BcsTypeOptions<T, Input>) {
	const type = new BcsType<T, Input>({
		...options,
		serialize,
		write: (value, writer) => {
			for (const byte of type.serialize(value).toBytes()) {
				writer.write8(byte);
			}
		},
	});

	return type;
}

export function stringLikeBcsType({
	toBytes,
	fromBytes,
	...options
}: {
	name: string;
	toBytes: (value: string) => Uint8Array;
	fromBytes: (bytes: Uint8Array) => string;
	serializedSize?: (value: string) => number | null;
} & BcsTypeOptions<string>) {
	return new BcsType<string>({
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
		validate: (value) => {
			if (typeof value !== 'string') {
				throw new TypeError(`Invalid ${options.name} value: ${value}. Expected string`);
			}
			options.validate?.(value);
		},
	});
}

export function lazyBcsType<T, Input>(cb: () => BcsType<T, Input>) {
	let lazyType: BcsType<T, Input> | null = null;
	function getType() {
		if (!lazyType) {
			lazyType = cb();
		}
		return lazyType;
	}

	return new BcsType<T, Input>({
		name: 'lazy' as never,
		read: (data) => getType().read(data),
		serializedSize: (value) => getType().serializedSize(value),
		write: (value, writer) => getType().write(value, writer),
		serialize: (value, options) => getType().serialize(value, options).toBytes(),
	});
}
