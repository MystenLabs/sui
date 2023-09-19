// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { BcsReader } from './reader';
import { ulebEncode } from './uleb';
import { BcsWriter, BcsWriterOptions } from './writer';

export interface BcsTypeOptions<T, Input = T> {
	name?: string;
	validate?: (value: Input) => void;
}

export class BcsType<T, Input = T> {
	name: string;
	read: (reader: BcsReader) => T;
	serializedSize: (value: Input, options?: BcsWriterOptions) => number | null;
	validate: (value: Input) => void;
	protected _write: (value: Input, writer: BcsWriter) => void;
	protected _serialize: (value: Input, options?: BcsWriterOptions) => Uint8Array;

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
		this._write = options.write;
		this._serialize =
			options.serialize ??
			((value, options) => {
				const writer = new BcsWriter({ size: this.serializedSize(value) ?? undefined, ...options });
				this._write(value, writer);
				return writer.toBytes();
			});

		this.validate = options.validate ?? (() => {});
	}

	write(value: Input, writer: BcsWriter) {
		this.validate(value);
		this._write(value, writer);
	}

	serialize(value: Input, options?: BcsWriterOptions) {
		this.validate(value);
		return this._serialize(value, options);
	}

	parse(bytes: Uint8Array): T {
		const reader = new BcsReader(bytes);
		return this.read(reader);
	}

	transform<T2, Input2>({
		name,
		input,
		output,
	}: {
		input: (val: Input2) => Input;
		output: (value: T) => T2;
	} & BcsTypeOptions<T2, Input2>) {
		return new BcsType<T2, Input2>({
			name: name ?? this.name,
			read: (reader) => output(this.read(reader)),
			write: (value, writer) => this._write(input(value), writer),
			serializedSize: (value) => this.serializedSize(input(value)),
			serialize: (value, options) => this._serialize(input(value), options),
			validate: (value) => this.validate(input(value)),
		});
	}
}

export class FixedSizeBcsType<T, Input = T> extends BcsType<T, Input> {
	size: number;

	constructor({
		size,
		...options
	}: {
		name: string;
		size: number;
		read: (reader: BcsReader) => T;
		write: (value: Input, writer: BcsWriter) => void;
	} & BcsTypeOptions<T, Input>) {
		super({
			...options,
			serializedSize: () => size,
		});
		this.size = size;
	}

	parse(bytes: Uint8Array): T {
		if (bytes.length < this.size) {
			throw new Error(`Invalid length: ${bytes.length} < ${this.size}`);
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
		name: string;
		size: number;
		readMethod: `read${8 | 16 | 32}`;
		writeMethod: `write${8 | 16 | 32}`;
		maxValue: number;
	} & BcsTypeOptions<number, number>) {
		super({
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
}

export class BigUIntBcsType extends FixedSizeBcsType<bigint, string | number | bigint> {
	constructor({
		readMethod,
		writeMethod,
		...options
	}: {
		name: string;
		size: number;
		readMethod: `read${64 | 128 | 256}`;
		writeMethod: `write${64 | 128 | 256}`;
		maxValue: bigint;
	} & BcsTypeOptions<bigint, number | bigint>) {
		super({
			...options,
			read: (reader) => BigInt(reader[readMethod]()),
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
}

export class DynamicSizeBcsType<T, Input = T> extends BcsType<T, Input> {
	constructor({
		serialize,
		...options
	}: {
		name: string;
		read: (reader: BcsReader) => T;
		serialize: (value: Input, options?: BcsWriterOptions) => Uint8Array;
	} & BcsTypeOptions<T, Input>) {
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
		name: string;
		toBytes: (value: string) => Uint8Array;
		fromBytes: (bytes: Uint8Array) => string;
		serializedSize?: (value: string) => number | null;
	} & BcsTypeOptions<string>) {
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
			validate: (value) => {
				if (typeof value !== 'string') {
					throw new TypeError(`Invalid ${options.name} value: ${value}. Expected string`);
				}
				options.validate?.(value);
			},
		});
	}
}

export class LazyBcsType<T, Input> extends BcsType<T, Input> {
	lazyType: BcsType<T, Input> | null = null;
	cb: () => BcsType<T, Input>;

	constructor(cb: () => BcsType<T, Input>) {
		super({
			name: 'lazy' as never,
			read: (data) => this.getType().read(data),
			serializedSize: (value) => this.getType().serializedSize(value),
			write: (value, writer) => this.getType().write(value, writer),
			serialize: (value, options) => this.getType().serialize(value, options),
		});

		this.cb = cb;
	}

	getType() {
		if (!this.lazyType) {
			this.lazyType = this.cb();
			this.name = this.lazyType.name;
		}
		return this.lazyType;
	}
}
