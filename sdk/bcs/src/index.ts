// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/*
 * BCS implementation {@see https://github.com/diem/bcs } for JavaScript.
 * Intended to be used for Move applications; supports both NodeJS and browser.
 *
 * For more details and examples {@see README.md }.
 *
 * @module bcs
 * @property {BcsReader}
 */

import { toB64, fromB64 } from './b64';
import { toHEX, fromHEX } from './hex';
import bs58 from 'bs58';

const SUI_ADDRESS_LENGTH = 32;

function toLittleEndian(bigint: bigint, size: number) {
	let result = new Uint8Array(size);
	let i = 0;
	while (bigint > 0) {
		result[i] = Number(bigint % BigInt(256));
		bigint = bigint / BigInt(256);
		i += 1;
	}
	return result;
}

const toB58 = (buffer: Uint8Array) => bs58.encode(buffer);
const fromB58 = (str: string) => bs58.decode(str);

// Re-export all encoding dependencies.
export { toB58, fromB58, toB64, fromB64, fromHEX, toHEX };

/**
 * Supported encodings.
 * Used in `Reader.toString()` as well as in `decodeStr` and `encodeStr` functions.
 */
export type Encoding = 'base58' | 'base64' | 'hex';

/**
 * Allows for array definitions for names.
 * @example
 * ```
 * bcs.registerStructType(['vector', BCS.STRING], ...);
 * // equals
 * bcs.registerStructType('vector<string>', ...);
 * ```
 */
export type TypeName = string | [string, ...(TypeName | string)[]];

/**
 * Class used for reading BCS data chunk by chunk. Meant to be used
 * by some wrapper, which will make sure that data is valid and is
 * matching the desired format.
 *
 * @example
 * // data for this example is:
 * // { a: u8, b: u32, c: bool, d: u64 }
 *
 * let reader = new BcsReader("647f1a060001ffffe7890423c78a050102030405");
 * let field1 = reader.read8();
 * let field2 = reader.read32();
 * let field3 = reader.read8() === '1'; // bool
 * let field4 = reader.read64();
 * // ....
 *
 * Reading vectors is another deal in bcs. To read a vector, you first need to read
 * its length using {@link readULEB}. Here's an example:
 * @example
 * // data encoded: { field: [1, 2, 3, 4, 5] }
 * let reader = new BcsReader("050102030405");
 * let vec_length = reader.readULEB();
 * let elements = [];
 * for (let i = 0; i < vec_length; i++) {
 *   elements.push(reader.read8());
 * }
 * console.log(elements); // [1,2,3,4,5]
 *
 * @param {String} data HEX-encoded data (serialized BCS)
 */
export class BcsReader {
	private dataView: DataView;
	private bytePosition: number = 0;

	/**
	 * @param {Uint8Array} data Data to use as a buffer.
	 */
	constructor(data: Uint8Array) {
		this.dataView = new DataView(data.buffer);
	}
	/**
	 * Shift current cursor position by `bytes`.
	 *
	 * @param {Number} bytes Number of bytes to
	 * @returns {this} Self for possible chaining.
	 */
	shift(bytes: number) {
		this.bytePosition += bytes;
		return this;
	}
	/**
	 * Read U8 value from the buffer and shift cursor by 1.
	 * @returns
	 */
	read8(): number {
		let value = this.dataView.getUint8(this.bytePosition);
		this.shift(1);
		return value;
	}
	/**
	 * Read U16 value from the buffer and shift cursor by 2.
	 * @returns
	 */
	read16(): number {
		let value = this.dataView.getUint16(this.bytePosition, true);
		this.shift(2);
		return value;
	}
	/**
	 * Read U32 value from the buffer and shift cursor by 4.
	 * @returns
	 */
	read32(): number {
		let value = this.dataView.getUint32(this.bytePosition, true);
		this.shift(4);
		return value;
	}
	/**
	 * Read U64 value from the buffer and shift cursor by 8.
	 * @returns
	 */
	read64(): string {
		let value1 = this.read32();
		let value2 = this.read32();

		let result = value2.toString(16) + value1.toString(16).padStart(8, '0');

		return BigInt('0x' + result).toString(10);
	}
	/**
	 * Read U128 value from the buffer and shift cursor by 16.
	 */
	read128(): string {
		let value1 = BigInt(this.read64());
		let value2 = BigInt(this.read64());
		let result = value2.toString(16) + value1.toString(16).padStart(16, '0');

		return BigInt('0x' + result).toString(10);
	}
	/**
	 * Read U128 value from the buffer and shift cursor by 32.
	 * @returns
	 */
	read256(): string {
		let value1 = BigInt(this.read128());
		let value2 = BigInt(this.read128());
		let result = value2.toString(16) + value1.toString(16).padStart(32, '0');

		return BigInt('0x' + result).toString(10);
	}
	/**
	 * Read `num` number of bytes from the buffer and shift cursor by `num`.
	 * @param num Number of bytes to read.
	 */
	readBytes(num: number): Uint8Array {
		let start = this.bytePosition + this.dataView.byteOffset;
		let value = new Uint8Array(this.dataView.buffer, start, num);

		this.shift(num);

		return value;
	}
	/**
	 * Read ULEB value - an integer of varying size. Used for enum indexes and
	 * vector lengths.
	 * @returns {Number} The ULEB value.
	 */
	readULEB(): number {
		let start = this.bytePosition + this.dataView.byteOffset;
		let buffer = new Uint8Array(this.dataView.buffer, start);
		let { value, length } = ulebDecode(buffer);

		this.shift(length);

		return value;
	}
	/**
	 * Read a BCS vector: read a length and then apply function `cb` X times
	 * where X is the length of the vector, defined as ULEB in BCS bytes.
	 * @param cb Callback to process elements of vector.
	 * @returns {Array<Any>} Array of the resulting values, returned by callback.
	 */
	readVec(cb: (reader: BcsReader, i: number, length: number) => any): any[] {
		let length = this.readULEB();
		let result = [];
		for (let i = 0; i < length; i++) {
			result.push(cb(this, i, length));
		}
		return result;
	}
}

/**
 * Class used to write BCS data into a buffer. Initializer requires
 * some size of a buffer to init; default value for this buffer is 1KB.
 *
 * Most methods are chainable, so it is possible to write them in one go.
 *
 * @example
 * let serialized = new BcsWriter()
 *   .write8(10)
 *   .write32(1000000)
 *   .write64(10000001000000)
 *   .hex();
 */

interface BcsWriterOptions {
	/** The initial size (in bytes) of the buffer tht will be allocated */
	size?: number;
	/** The maximum size (in bytes) that the buffer is allowed to grow to */
	maxSize?: number;
	/** The amount of bytes that will be allocated whenever additional memory is required */
	allocateSize?: number;
}

export class BcsWriter {
	private dataView: DataView;
	private bytePosition: number = 0;
	private size: number;
	private maxSize: number;
	private allocateSize: number;

	constructor({ size = 1024, maxSize, allocateSize = 1024 }: BcsWriterOptions = {}) {
		this.size = size;
		this.maxSize = maxSize || size;
		this.allocateSize = allocateSize;
		this.dataView = new DataView(new ArrayBuffer(size));
	}

	private ensureSizeOrGrow(bytes: number) {
		const requiredSize = this.bytePosition + bytes;
		if (requiredSize > this.size) {
			const nextSize = Math.min(this.maxSize, this.size + this.allocateSize);
			if (requiredSize > nextSize) {
				throw new Error(
					`Attempting to serialize to BCS, but buffer does not have enough size. Allocated size: ${this.size}, Max size: ${this.maxSize}, Required size: ${requiredSize}`,
				);
			}

			this.size = nextSize;
			const nextBuffer = new ArrayBuffer(this.size);
			new Uint8Array(nextBuffer).set(new Uint8Array(this.dataView.buffer));
			this.dataView = new DataView(nextBuffer);
		}
	}

	/**
	 * Shift current cursor position by `bytes`.
	 *
	 * @param {Number} bytes Number of bytes to
	 * @returns {this} Self for possible chaining.
	 */
	shift(bytes: number): this {
		this.bytePosition += bytes;
		return this;
	}
	/**
	 * Write a U8 value into a buffer and shift cursor position by 1.
	 * @param {Number} value Value to write.
	 * @returns {this}
	 */
	write8(value: number | bigint): this {
		this.ensureSizeOrGrow(1);
		this.dataView.setUint8(this.bytePosition, Number(value));
		return this.shift(1);
	}
	/**
	 * Write a U16 value into a buffer and shift cursor position by 2.
	 * @param {Number} value Value to write.
	 * @returns {this}
	 */
	write16(value: number | bigint): this {
		this.ensureSizeOrGrow(2);
		this.dataView.setUint16(this.bytePosition, Number(value), true);
		return this.shift(2);
	}
	/**
	 * Write a U32 value into a buffer and shift cursor position by 4.
	 * @param {Number} value Value to write.
	 * @returns {this}
	 */
	write32(value: number | bigint): this {
		this.ensureSizeOrGrow(4);
		this.dataView.setUint32(this.bytePosition, Number(value), true);
		return this.shift(4);
	}
	/**
	 * Write a U64 value into a buffer and shift cursor position by 8.
	 * @param {bigint} value Value to write.
	 * @returns {this}
	 */
	write64(value: number | bigint): this {
		toLittleEndian(BigInt(value), 8).forEach((el) => this.write8(el));

		return this;
	}
	/**
	 * Write a U128 value into a buffer and shift cursor position by 16.
	 *
	 * @param {bigint} value Value to write.
	 * @returns {this}
	 */
	write128(value: number | bigint): this {
		toLittleEndian(BigInt(value), 16).forEach((el) => this.write8(el));

		return this;
	}
	/**
	 * Write a U256 value into a buffer and shift cursor position by 16.
	 *
	 * @param {bigint} value Value to write.
	 * @returns {this}
	 */
	write256(value: number | bigint): this {
		toLittleEndian(BigInt(value), 32).forEach((el) => this.write8(el));

		return this;
	}
	/**
	 * Write a ULEB value into a buffer and shift cursor position by number of bytes
	 * written.
	 * @param {Number} value Value to write.
	 * @returns {this}
	 */
	writeULEB(value: number): this {
		ulebEncode(value).forEach((el) => this.write8(el));
		return this;
	}
	/**
	 * Write a vector into a buffer by first writing the vector length and then calling
	 * a callback on each passed value.
	 *
	 * @param {Array<Any>} vector Array of elements to write.
	 * @param {WriteVecCb} cb Callback to call on each element of the vector.
	 * @returns {this}
	 */
	writeVec(vector: any[], cb: (writer: BcsWriter, el: any, i: number, len: number) => void): this {
		this.writeULEB(vector.length);
		Array.from(vector).forEach((el, i) => cb(this, el, i, vector.length));
		return this;
	}

	/**
	 * Adds support for iterations over the object.
	 * @returns {Uint8Array}
	 */
	*[Symbol.iterator](): Iterator<number, Iterable<number>> {
		for (let i = 0; i < this.bytePosition; i++) {
			yield this.dataView.getUint8(i);
		}
		return this.toBytes();
	}

	/**
	 * Get underlying buffer taking only value bytes (in case initial buffer size was bigger).
	 * @returns {Uint8Array} Resulting bcs.
	 */
	toBytes(): Uint8Array {
		return new Uint8Array(this.dataView.buffer.slice(0, this.bytePosition));
	}

	/**
	 * Represent data as 'hex' or 'base64'
	 * @param encoding Encoding to use: 'base64' or 'hex'
	 */
	toString(encoding: Encoding): string {
		return encodeStr(this.toBytes(), encoding);
	}
}

// Helper utility: write number as an ULEB array.
// Original code is taken from: https://www.npmjs.com/package/uleb128 (no longer exists)
function ulebEncode(num: number): number[] {
	let arr = [];
	let len = 0;

	if (num === 0) {
		return [0];
	}

	while (num > 0) {
		arr[len] = num & 0x7f;
		if ((num >>= 7)) {
			arr[len] |= 0x80;
		}
		len += 1;
	}

	return arr;
}

// Helper utility: decode ULEB as an array of numbers.
// Original code is taken from: https://www.npmjs.com/package/uleb128 (no longer exists)
function ulebDecode(arr: number[] | Uint8Array): {
	value: number;
	length: number;
} {
	let total = 0;
	let shift = 0;
	let len = 0;

	// eslint-disable-next-line no-constant-condition
	while (true) {
		let byte = arr[len];
		len += 1;
		total |= (byte & 0x7f) << shift;
		if ((byte & 0x80) === 0) {
			break;
		}
		shift += 7;
	}

	return {
		value: total,
		length: len,
	};
}

/**
 * Set of methods that allows data encoding/decoding as standalone
 * BCS value or a part of a composed structure/vector.
 */
export interface TypeInterface {
	encode: (
		self: BCS,
		data: any,
		options: BcsWriterOptions | undefined,
		typeParams: TypeName[],
	) => BcsWriter;
	decode: (self: BCS, data: Uint8Array, typeParams: TypeName[]) => any;

	_encodeRaw: (
		writer: BcsWriter,
		data: any,
		typeParams: TypeName[],
		typeMap: { [key: string]: TypeName },
	) => BcsWriter;
	_decodeRaw: (
		reader: BcsReader,
		typeParams: TypeName[],
		typeMap: { [key: string]: TypeName },
	) => any;
}

/**
 * Struct type definition. Used as input format in BcsConfig.types
 * as well as an argument type for `bcs.registerStructType`.
 */
export type StructTypeDefinition = {
	[key: string]: TypeName | StructTypeDefinition;
};

/**
 * Enum type definition. Used as input format in BcsConfig.types
 * as well as an argument type for `bcs.registerEnumType`.
 *
 * Value can be either `string` when invariant has a type or `null`
 * when invariant is empty.
 *
 * @example
 * bcs.registerEnumType('Option<T>', {
 *   some: 'T',
 *   none: null
 * });
 */
export type EnumTypeDefinition = {
	[key: string]: TypeName | StructTypeDefinition | null;
};

/**
 * Configuration that is passed into BCS constructor.
 */
export type BcsConfig = {
	/**
	 * Defines type name for the vector / array type.
	 * In Move: `vector<T>` or `vector`.
	 */
	vectorType: string;
	/**
	 * Address length. Varies depending on a platform and
	 * has to be specified for the `address` type.
	 */
	addressLength: number;

	/**
	 * Custom encoding for address. Supported values are
	 * either 'hex' or 'base64'.
	 */
	addressEncoding?: 'hex' | 'base64';
	/**
	 * Opening and closing symbol for type parameters. Can be
	 * any pair of symbols (eg `['(', ')']`); default value follows
	 * Rust and Move: `<` and `>`.
	 */
	genericSeparators?: [string, string];
	/**
	 * Type definitions for the BCS. This field allows spawning
	 * BCS instance from JSON or another prepared configuration.
	 * Optional.
	 */
	types?: {
		structs?: { [key: string]: StructTypeDefinition };
		enums?: { [key: string]: EnumTypeDefinition };
		aliases?: { [key: string]: string };
	};
	/**
	 * Whether to auto-register primitive types on launch.
	 */
	withPrimitives?: boolean;
};

/**
 * BCS implementation for Move types and few additional built-ins.
 */
export class BCS {
	// Prefefined types constants
	static readonly U8: string = 'u8';
	static readonly U16: string = 'u16';
	static readonly U32: string = 'u32';
	static readonly U64: string = 'u64';
	static readonly U128: string = 'u128';
	static readonly U256: string = 'u256';
	static readonly BOOL: string = 'bool';
	static readonly VECTOR: string = 'vector';
	static readonly ADDRESS: string = 'address';
	static readonly STRING: string = 'string';
	static readonly HEX: string = 'hex-string';
	static readonly BASE58: string = 'base58-string';
	static readonly BASE64: string = 'base64-string';

	/**
	 * Map of kind `TypeName => TypeInterface`. Holds all
	 * callbacks for (de)serialization of every registered type.
	 *
	 * If the value stored is a string, it is treated as an alias.
	 */
	public types: Map<string, TypeInterface | string> = new Map();

	/**
	 * Stored BcsConfig for the current instance of BCS.
	 */
	protected schema: BcsConfig;

	/**
	 * Count temp keys to generate a new one when requested.
	 */
	protected counter: number = 0;

	/**
	 * Name of the key to use for temporary struct definitions.
	 * Returns a temp key + index (for a case when multiple temp
	 * structs are processed).
	 */
	private tempKey() {
		return `bcs-struct-${++this.counter}`;
	}

	/**
	 * Construct a BCS instance with a prepared schema.
	 *
	 * @param schema A prepared schema with type definitions
	 * @param withPrimitives Whether to register primitive types by default
	 */
	constructor(schema: BcsConfig | BCS) {
		// if BCS instance is passed -> clone its schema
		if (schema instanceof BCS) {
			this.schema = schema.schema;
			this.types = new Map(schema.types);
			return;
		}

		this.schema = schema;

		// Register address type under key 'address'.
		this.registerAddressType(BCS.ADDRESS, schema.addressLength, schema.addressEncoding);
		this.registerVectorType(schema.vectorType);

		// Register struct types if they were passed.
		if (schema.types && schema.types.structs) {
			for (let name of Object.keys(schema.types.structs)) {
				this.registerStructType(name, schema.types.structs[name]);
			}
		}

		// Register enum types if they were passed.
		if (schema.types && schema.types.enums) {
			for (let name of Object.keys(schema.types.enums)) {
				this.registerEnumType(name, schema.types.enums[name]);
			}
		}

		// Register aliases if they were passed.
		if (schema.types && schema.types.aliases) {
			for (let name of Object.keys(schema.types.aliases)) {
				this.registerAlias(name, schema.types.aliases[name]);
			}
		}

		if (schema.withPrimitives !== false) {
			registerPrimitives(this);
		}
	}

	/**
	 * Serialize data into bcs.
	 *
	 * @example
	 * bcs.registerVectorType('vector<u8>', 'u8');
	 *
	 * let serialized = BCS
	 *   .set('vector<u8>', [1,2,3,4,5,6])
	 *   .toBytes();
	 *
	 * console.assert(toHex(serialized) === '06010203040506');
	 *
	 * @param type Name of the type to serialize (must be registered) or a struct type.
	 * @param data Data to serialize.
	 * @param size Serialization buffer size. Default 1024 = 1KB.
	 * @return A BCS reader instance. Usually you'd want to call `.toBytes()`
	 */
	public ser(
		type: TypeName | StructTypeDefinition,
		data: any,
		options?: BcsWriterOptions,
	): BcsWriter {
		if (typeof type === 'string' || Array.isArray(type)) {
			const { name, params } = this.parseTypeName(type);
			return this.getTypeInterface(name).encode(this, data, options, params as string[]);
		}

		// Quick serialization without registering the type in the main struct.
		if (typeof type === 'object') {
			const key = this.tempKey();
			const temp = new BCS(this);
			return temp.registerStructType(key, type).ser(key, data, options);
		}

		throw new Error(`Incorrect type passed into the '.ser()' function. \n${JSON.stringify(type)}`);
	}

	/**
	 * Deserialize BCS into a JS type.
	 *
	 * @example
	 * let num = bcs.ser('u64', '4294967295').toString('hex');
	 * let deNum = bcs.de('u64', num, 'hex');
	 * console.assert(deNum.toString(10) === '4294967295');
	 *
	 * @param type Name of the type to deserialize (must be registered) or a struct type definition.
	 * @param data Data to deserialize.
	 * @param encoding Optional - encoding to use if data is of type String
	 * @return Deserialized data.
	 */
	public de(
		type: TypeName | StructTypeDefinition,
		data: Uint8Array | string,
		encoding?: Encoding,
	): any {
		if (typeof data === 'string') {
			if (encoding) {
				data = decodeStr(data, encoding);
			} else {
				throw new Error('To pass a string to `bcs.de`, specify encoding');
			}
		}

		// In case the type specified is already registered.
		if (typeof type === 'string' || Array.isArray(type)) {
			const { name, params } = this.parseTypeName(type);
			return this.getTypeInterface(name).decode(this, data, params as string[]);
		}

		// Deserialize without registering a type using a temporary clone.
		if (typeof type === 'object') {
			const temp = new BCS(this);
			const key = this.tempKey();
			return temp.registerStructType(key, type).de(key, data, encoding);
		}

		throw new Error(`Incorrect type passed into the '.de()' function. \n${JSON.stringify(type)}`);
	}

	/**
	 * Check whether a `TypeInterface` has been loaded for a `type`.
	 * @param type Name of the type to check.
	 * @returns
	 */
	public hasType(type: string): boolean {
		return this.types.has(type);
	}

	/**
	 * Create an alias for a type.
	 * WARNING: this can potentially lead to recursion
	 * @param name Alias to use
	 * @param forType Type to reference
	 * @returns
	 *
	 * @example
	 * ```
	 * let bcs = new BCS(getSuiMoveConfig());
	 * bcs.registerAlias('ObjectDigest', BCS.BASE58);
	 * let b58_digest = bcs.de('ObjectDigest', '<digest_bytes>', 'base64');
	 * ```
	 */
	public registerAlias(name: string, forType: string): BCS {
		this.types.set(name, forType);
		return this;
	}

	/**
	 * Method to register new types for BCS internal representation.
	 * For each registered type 2 callbacks must be specified and one is optional:
	 *
	 * - encodeCb(writer, data) - write a way to serialize data with BcsWriter;
	 * - decodeCb(reader) - write a way to deserialize data with BcsReader;
	 * - validateCb(data) - validate data - either return bool or throw an error
	 *
	 * @example
	 * // our type would be a string that consists only of numbers
	 * bcs.registerType('number_string',
	 *    (writer, data) => writer.writeVec(data, (w, el) => w.write8(el)),
	 *    (reader) => reader.readVec((r) => r.read8()).join(''), // read each value as u8
	 *    (value) => /[0-9]+/.test(value) // test that it has at least one digit
	 * );
	 * console.log(Array.from(bcs.ser('number_string', '12345').toBytes()) == [5,1,2,3,4,5]);
	 *
	 * @param name
	 * @param encodeCb Callback to encode a value.
	 * @param decodeCb Callback to decode a value.
	 * @param validateCb Optional validator Callback to check type before serialization.
	 */
	public registerType(
		typeName: TypeName,
		encodeCb: (
			writer: BcsWriter,
			data: any,
			typeParams: TypeName[],
			typeMap: { [key: string]: TypeName },
		) => BcsWriter,
		decodeCb: (
			reader: BcsReader,
			typeParams: TypeName[],
			typeMap: { [key: string]: TypeName },
		) => any,
		validateCb: (data: any) => boolean = () => true,
	): BCS {
		const { name, params: generics } = this.parseTypeName(typeName);

		this.types.set(name, {
			encode(self: BCS, data, options: BcsWriterOptions, typeParams) {
				const typeMap = (generics as string[]).reduce((acc: any, value: string, index) => {
					return Object.assign(acc, { [value]: typeParams[index] });
				}, {});

				return this._encodeRaw.call(self, new BcsWriter(options), data, typeParams, typeMap);
			},
			decode(self: BCS, data, typeParams) {
				const typeMap = (generics as string[]).reduce((acc: any, value: string, index) => {
					return Object.assign(acc, { [value]: typeParams[index] });
				}, {});

				return this._decodeRaw.call(self, new BcsReader(data), typeParams, typeMap);
			},

			// these methods should always be used with caution as they require pre-defined
			// reader and writer and mainly exist to allow multi-field (de)serialization;
			_encodeRaw(writer, data, typeParams, typeMap) {
				if (validateCb(data)) {
					return encodeCb.call(this, writer, data, typeParams, typeMap);
				} else {
					throw new Error(`Validation failed for type ${name}, data: ${data}`);
				}
			},
			_decodeRaw(reader, typeParams, typeMap) {
				return decodeCb.call(this, reader, typeParams, typeMap);
			},
		} as TypeInterface);

		return this;
	}

	/**
	 * Register an address type which is a sequence of U8s of specified length.
	 * @example
	 * bcs.registerAddressType('address', SUI_ADDRESS_LENGTH);
	 * let addr = bcs.de('address', 'c3aca510c785c7094ac99aeaa1e69d493122444df50bb8a99dfa790c654a79af');
	 *
	 * @param name Name of the address type.
	 * @param length Byte length of the address.
	 * @param encoding Encoding to use for the address type
	 * @returns
	 */
	public registerAddressType(name: string, length: number, encoding: Encoding | void = 'hex'): BCS {
		switch (encoding) {
			case 'base64':
				return this.registerType(
					name,
					function encodeAddress(writer, data: string) {
						return fromB64(data).reduce((writer, el) => writer.write8(el), writer);
					},
					function decodeAddress(reader) {
						return toB64(reader.readBytes(length));
					},
				);
			case 'hex':
				return this.registerType(
					name,
					function encodeAddress(writer, data: string) {
						return fromHEX(data).reduce((writer, el) => writer.write8(el), writer);
					},
					function decodeAddress(reader) {
						return toHEX(reader.readBytes(length));
					},
				);
			default:
				throw new Error('Unsupported encoding! Use either hex or base64');
		}
	}

	/**
	 * Register custom vector type inside the bcs.
	 *
	 * @example
	 * bcs.registerVectorType('vector<T>'); // generic registration
	 * let array = bcs.de('vector<u8>', '06010203040506', 'hex'); // [1,2,3,4,5,6];
	 * let again = bcs.ser('vector<u8>', [1,2,3,4,5,6]).toString('hex');
	 *
	 * @param name Name of the type to register
	 * @param elementType Optional name of the inner type of the vector
	 * @return Returns self for chaining.
	 */
	private registerVectorType(typeName: string): BCS {
		let { name, params } = this.parseTypeName(typeName);
		if (params.length > 1) {
			throw new Error('Vector can have only one type parameter; got ' + name);
		}

		return this.registerType(
			typeName,
			function encodeVector(
				this: BCS,
				writer: BcsWriter,
				data: any[],
				typeParams: TypeName[],
				typeMap,
			) {
				return writer.writeVec(data, (writer, el) => {
					let elementType: TypeName = typeParams[0];
					if (!elementType) {
						throw new Error(`Incorrect number of type parameters passed a to vector '${typeName}'`);
					}

					let { name, params } = this.parseTypeName(elementType);
					if (this.hasType(name)) {
						return this.getTypeInterface(name)._encodeRaw.call(this, writer, el, params, typeMap);
					}

					if (!(name in typeMap)) {
						throw new Error(
							`Unable to find a matching type definition for ${name} in vector; make sure you passed a generic`,
						);
					}

					let { name: innerName, params: innerParams } = this.parseTypeName(typeMap[name]);

					return this.getTypeInterface(innerName)._encodeRaw.call(
						this,
						writer,
						el,
						innerParams,
						typeMap,
					);
				});
			},
			function decodeVector(this: BCS, reader: BcsReader, typeParams, typeMap) {
				return reader.readVec((reader) => {
					let elementType: TypeName = typeParams[0];
					if (!elementType) {
						throw new Error(`Incorrect number of type parameters passed to a vector '${typeName}'`);
					}

					let { name, params } = this.parseTypeName(elementType);
					if (this.hasType(name)) {
						return this.getTypeInterface(name)._decodeRaw.call(this, reader, params, typeMap);
					}

					if (!(name in typeMap)) {
						throw new Error(
							`Unable to find a matching type definition for ${name} in vector; make sure you passed a generic`,
						);
					}

					let { name: innerName, params: innerParams } = this.parseTypeName(typeMap[name]);

					return this.getTypeInterface(innerName)._decodeRaw.call(
						this,
						reader,
						innerParams,
						typeMap,
					);
				});
			},
		);
	}

	/**
	 * Safe method to register a custom Move struct. The first argument is a name of the
	 * struct which is only used on the FrontEnd and has no affect on serialization results,
	 * and the second is a struct description passed as an Object.
	 *
	 * The description object MUST have the same order on all of the platforms (ie in Move
	 * or in Rust).
	 *
	 * @example
	 * // Move / Rust struct
	 * // struct Coin {
	 * //   value: u64,
	 * //   owner: vector<u8>, // name // Vec<u8> in Rust
	 * //   is_locked: bool,
	 * // }
	 *
	 * bcs.registerStructType('Coin', {
	 *   value: bcs.U64,
	 *   owner: bcs.STRING,
	 *   is_locked: bcs.BOOL
	 * });
	 *
	 * // Created in Rust with diem/bcs
	 * // let rust_bcs_str = '80d1b105600000000e4269672057616c6c65742047757900';
	 * let rust_bcs_str = [ // using an Array here as BCS works with Uint8Array
	 *  128, 209, 177,   5,  96,  0,  0,
	 *    0,  14,  66, 105, 103, 32, 87,
	 *   97, 108, 108, 101, 116, 32, 71,
	 *  117, 121,   0
	 * ];
	 *
	 * // Let's encode the value as well
	 * let test_set = bcs.ser('Coin', {
	 *   owner: 'Big Wallet Guy',
	 *   value: '412412400000',
	 *   is_locked: false,
	 * });
	 *
	 * console.assert(Array.from(test_set.toBytes()) === rust_bcs_str, 'Whoopsie, result mismatch');
	 *
	 * @param name Name of the type to register.
	 * @param fields Fields of the struct. Must be in the correct order.
	 * @return Returns BCS for chaining.
	 */
	public registerStructType(typeName: TypeName, fields: StructTypeDefinition): BCS {
		// When an Object is passed, we register it under a new key and store it
		// in the registered type system. This way we allow nested inline definitions.
		for (let key in fields) {
			let internalName = this.tempKey();
			let value = fields[key];

			// TODO: add a type guard here?
			if (!Array.isArray(value) && typeof value !== 'string') {
				fields[key] = internalName;
				this.registerStructType(internalName, value as StructTypeDefinition);
			}
		}

		let struct = Object.freeze(fields); // Make sure the order doesn't get changed

		// IMPORTANT: we need to store canonical order of fields for each registered
		// struct so we maintain it and allow developers to use any field ordering in
		// their code (and not cause mismatches based on field order).
		let canonicalOrder = Object.keys(struct);

		// Holds generics for the struct definition. At this stage we can check that
		// generic parameter matches the one defined in the struct.
		let { name: structName, params: generics } = this.parseTypeName(typeName);

		// Make sure all the types in the fields description are already known
		// and that all the field types are strings.
		return this.registerType(
			typeName,
			function encodeStruct(
				this: BCS,
				writer: BcsWriter,
				data: { [key: string]: any },
				typeParams,
				typeMap,
			) {
				if (!data || data.constructor !== Object) {
					throw new Error(`Expected ${structName} to be an Object, got: ${data}`);
				}

				if (typeParams.length !== generics.length) {
					throw new Error(
						`Incorrect number of generic parameters passed; expected: ${generics.length}, got: ${typeParams.length}`,
					);
				}

				// follow the canonical order when serializing
				for (let key of canonicalOrder) {
					if (!(key in data)) {
						throw new Error(`Struct ${structName} requires field ${key}:${struct[key]}`);
					}

					// Before deserializing, read the canonical field type.
					const { name: fieldType, params: fieldParams } = this.parseTypeName(
						struct[key] as TypeName,
					);

					// Check whether this type is a generic defined in this struct.
					// If it is -> read the type parameter matching its index.
					// If not - tread as a regular field.
					if (!generics.includes(fieldType)) {
						this.getTypeInterface(fieldType)._encodeRaw.call(
							this,
							writer,
							data[key],
							fieldParams,
							typeMap,
						);
					} else {
						const paramIdx = generics.indexOf(fieldType);
						let { name, params } = this.parseTypeName(typeParams[paramIdx]);

						// If the type from the type parameters already exists
						// and known -> proceed with type decoding.
						if (this.hasType(name)) {
							this.getTypeInterface(name)._encodeRaw.call(
								this,
								writer,
								data[key],
								params as string[],
								typeMap,
							);
							continue;
						}

						// Alternatively, if it's a global generic parameter...
						if (!(name in typeMap)) {
							throw new Error(
								`Unable to find a matching type definition for ${name} in ${structName}; make sure you passed a generic`,
							);
						}

						let { name: innerName, params: innerParams } = this.parseTypeName(typeMap[name]);
						this.getTypeInterface(innerName)._encodeRaw.call(
							this,
							writer,
							data[key],
							innerParams,
							typeMap,
						);
					}
				}
				return writer;
			},
			function decodeStruct(this: BCS, reader: BcsReader, typeParams, typeMap) {
				if (typeParams.length !== generics.length) {
					throw new Error(
						`Incorrect number of generic parameters passed; expected: ${generics.length}, got: ${typeParams.length}`,
					);
				}

				let result: { [key: string]: any } = {};
				for (let key of canonicalOrder) {
					const { name: fieldName, params: fieldParams } = this.parseTypeName(
						struct[key] as TypeName,
					);

					// if it's not a generic
					if (!generics.includes(fieldName)) {
						result[key] = this.getTypeInterface(fieldName)._decodeRaw.call(
							this,
							reader,
							fieldParams as string[],
							typeMap,
						);
					} else {
						const paramIdx = generics.indexOf(fieldName);
						let { name, params } = this.parseTypeName(typeParams[paramIdx]);

						// If the type from the type parameters already exists
						// and known -> proceed with type decoding.
						if (this.hasType(name)) {
							result[key] = this.getTypeInterface(name)._decodeRaw.call(
								this,
								reader,
								params,
								typeMap,
							);
							continue;
						}

						if (!(name in typeMap)) {
							throw new Error(
								`Unable to find a matching type definition for ${name} in ${structName}; make sure you passed a generic`,
							);
						}

						let { name: innerName, params: innerParams } = this.parseTypeName(typeMap[name]);
						result[key] = this.getTypeInterface(innerName)._decodeRaw.call(
							this,
							reader,
							innerParams,
							typeMap,
						);
					}
				}
				return result;
			},
		);
	}

	/**
	 * Safe method to register custom enum type where each invariant holds the value of another type.
	 * @example
	 * bcs.registerStructType('Coin', { value: 'u64' });
	 * bcs.registerEnumType('MyEnum', {
	 *  single: 'Coin',
	 *  multi: 'vector<Coin>',
	 *  empty: null
	 * });
	 *
	 * console.log(
	 *  bcs.de('MyEnum', 'AICWmAAAAAAA', 'base64'), // { single: { value: 10000000 } }
	 *  bcs.de('MyEnum', 'AQIBAAAAAAAAAAIAAAAAAAAA', 'base64')  // { multi: [ { value: 1 }, { value: 2 } ] }
	 * )
	 *
	 * // and serialization
	 * bcs.ser('MyEnum', { single: { value: 10000000 } }).toBytes();
	 * bcs.ser('MyEnum', { multi: [ { value: 1 }, { value: 2 } ] });
	 *
	 * @param name
	 * @param variants
	 */
	public registerEnumType(typeName: TypeName, variants: EnumTypeDefinition): BCS {
		// When an Object is passed, we register it under a new key and store it
		// in the registered type system. This way we allow nested inline definitions.
		for (let key in variants) {
			let internalName = this.tempKey();
			let value = variants[key];

			if (value !== null && !Array.isArray(value) && typeof value !== 'string') {
				variants[key] = internalName;
				this.registerStructType(internalName, value as StructTypeDefinition);
			}
		}

		let struct = Object.freeze(variants); // Make sure the order doesn't get changed

		// IMPORTANT: enum is an ordered type and we have to preserve ordering in BCS
		let canonicalOrder = Object.keys(struct);

		// Parse type parameters in advance to know the index of each generic parameter.
		let { name, params: canonicalTypeParams } = this.parseTypeName(typeName);

		return this.registerType(
			typeName,
			function encodeEnum(
				this: BCS,
				writer: BcsWriter,
				data: { [key: string]: any | null },
				typeParams,
				typeMap,
			) {
				if (!data) {
					throw new Error(`Unable to write enum "${name}", missing data.\nReceived: "${data}"`);
				}
				if (typeof data !== 'object') {
					throw new Error(
						`Incorrect data passed into enum "${name}", expected object with properties: "${canonicalOrder.join(
							' | ',
						)}".\nReceived: "${JSON.stringify(data)}"`,
					);
				}

				let key = Object.keys(data)[0];
				if (key === undefined) {
					throw new Error(`Empty object passed as invariant of the enum "${name}"`);
				}

				let orderByte = canonicalOrder.indexOf(key);
				if (orderByte === -1) {
					throw new Error(
						`Unknown invariant of the enum "${name}", allowed values: "${canonicalOrder.join(
							' | ',
						)}"; received "${key}"`,
					);
				}
				let invariant = canonicalOrder[orderByte];
				let invariantType = struct[invariant] as TypeName | null;

				// write order byte
				writer.write8(orderByte);

				// When { "key": null } - empty value for the invariant.
				if (invariantType === null) {
					return writer;
				}

				let paramIndex = canonicalTypeParams.indexOf(invariantType);
				let typeOrParam = paramIndex === -1 ? invariantType : typeParams[paramIndex];

				{
					let { name, params } = this.parseTypeName(typeOrParam);
					return this.getTypeInterface(name)._encodeRaw.call(
						this,
						writer,
						data[key],
						params,
						typeMap,
					);
				}
			},
			function decodeEnum(this: BCS, reader: BcsReader, typeParams, typeMap) {
				let orderByte = reader.readULEB();
				let invariant = canonicalOrder[orderByte];
				let invariantType = struct[invariant] as TypeName | null;

				if (orderByte === -1) {
					throw new Error(
						`Decoding type mismatch, expected enum "${name}" invariant index, received "${orderByte}"`,
					);
				}

				// Encode an empty value for the enum.
				if (invariantType === null) {
					return { [invariant]: true };
				}

				let paramIndex = canonicalTypeParams.indexOf(invariantType);
				let typeOrParam = paramIndex === -1 ? invariantType : typeParams[paramIndex];

				{
					let { name, params } = this.parseTypeName(typeOrParam);
					return {
						[invariant]: this.getTypeInterface(name)._decodeRaw.call(this, reader, params, typeMap),
					};
				}
			},
		);
	}
	/**
	 * Get a set of encoders/decoders for specific type.
	 * Mainly used to define custom type de/serialization logic.
	 *
	 * @param type
	 * @returns {TypeInterface}
	 */
	public getTypeInterface(type: string): TypeInterface {
		let typeInterface = this.types.get(type);

		// Special case - string means an alias.
		// Goes through the alias chain and tracks recursion.
		if (typeof typeInterface === 'string') {
			let chain: string[] = [];
			while (typeof typeInterface === 'string') {
				if (chain.includes(typeInterface)) {
					throw new Error(`Recursive definition found: ${chain.join(' -> ')} -> ${typeInterface}`);
				}
				chain.push(typeInterface);
				typeInterface = this.types.get(typeInterface);
			}
		}

		if (typeInterface === undefined) {
			throw new Error(`Type ${type} is not registered`);
		}

		return typeInterface;
	}

	/**
	 * Parse a type name and get the type's generics.
	 * @example
	 * let { typeName, typeParams } = parseTypeName('Option<Coin<SUI>>');
	 * // typeName: Option
	 * // typeParams: [ 'Coin<SUI>' ]
	 *
	 * @param name Name of the type to process
	 * @returns Object with typeName and typeParams listed as Array
	 */
	public parseTypeName(name: TypeName): {
		name: string;
		params: TypeName[];
	} {
		if (Array.isArray(name)) {
			let [typeName, ...params] = name;
			return { name: typeName, params };
		}

		if (typeof name !== 'string') {
			throw new Error(`Illegal type passed as a name of the type: ${name}`);
		}

		let [left, right] = this.schema.genericSeparators || ['<', '>'];

		let l_bound = name.indexOf(left);
		let r_bound = Array.from(name).reverse().indexOf(right);

		// if there are no generics - exit gracefully.
		if (l_bound === -1 && r_bound === -1) {
			return { name: name, params: [] };
		}

		// if one of the bounds is not defined - throw an Error.
		if (l_bound === -1 || r_bound === -1) {
			throw new Error(`Unclosed generic in name '${name}'`);
		}

		let typeName = name.slice(0, l_bound);
		let params = splitGenericParameters(
			name.slice(l_bound + 1, name.length - r_bound - 1),
			this.schema.genericSeparators,
		);

		return { name: typeName, params };
	}
}

/**
 * Encode data with either `hex` or `base64`.
 *
 * @param {Uint8Array} data Data to encode.
 * @param {String} encoding Encoding to use: base64 or hex
 * @return {String} Encoded value.
 */
export function encodeStr(data: Uint8Array, encoding: Encoding): string {
	switch (encoding) {
		case 'base58':
			return toB58(data);
		case 'base64':
			return toB64(data);
		case 'hex':
			return toHEX(data);
		default:
			throw new Error('Unsupported encoding, supported values are: base64, hex');
	}
}

/**
 * Decode either `base64` or `hex` data.
 *
 * @param {String} data Data to encode.
 * @param {String} encoding Encoding to use: base64 or hex
 * @return {Uint8Array} Encoded value.
 */
export function decodeStr(data: string, encoding: Encoding): Uint8Array {
	switch (encoding) {
		case 'base58':
			return fromB58(data);
		case 'base64':
			return fromB64(data);
		case 'hex':
			return fromHEX(data);
		default:
			throw new Error('Unsupported encoding, supported values are: base64, hex');
	}
}

/**
 * Register the base set of primitive and common types.
 * Is called in the `BCS` constructor automatically but can
 * be ignored if the `withPrimitives` argument is not set.
 */
export function registerPrimitives(bcs: BCS): void {
	bcs.registerType(
		BCS.U8,
		function (writer: BcsWriter, data) {
			return writer.write8(data);
		},
		function (reader: BcsReader) {
			return reader.read8();
		},
		(u8) => u8 < 256,
	);

	bcs.registerType(
		BCS.U16,
		function (writer: BcsWriter, data) {
			return writer.write16(data);
		},
		function (reader: BcsReader) {
			return reader.read16();
		},
		(u16) => u16 < 65536,
	);

	bcs.registerType(
		BCS.U32,
		function (writer: BcsWriter, data) {
			return writer.write32(data);
		},
		function (reader: BcsReader) {
			return reader.read32();
		},
		(u32) => u32 <= 4294967296n,
	);

	bcs.registerType(
		BCS.U64,
		function (writer: BcsWriter, data) {
			return writer.write64(data);
		},
		function (reader: BcsReader) {
			return reader.read64();
		},
	);

	bcs.registerType(
		BCS.U128,
		function (writer: BcsWriter, data: bigint) {
			return writer.write128(data);
		},
		function (reader: BcsReader) {
			return reader.read128();
		},
	);

	bcs.registerType(
		BCS.U256,
		function (writer: BcsWriter, data) {
			return writer.write256(data);
		},
		function (reader: BcsReader) {
			return reader.read256();
		},
	);

	bcs.registerType(
		BCS.BOOL,
		function (writer: BcsWriter, data) {
			return writer.write8(data);
		},
		function (reader: BcsReader) {
			return reader.read8().toString(10) === '1';
		},
	);

	bcs.registerType(
		BCS.STRING,
		function (writer: BcsWriter, data: string) {
			return writer.writeVec(Array.from(data), (writer, el) => writer.write8(el.charCodeAt(0)));
		},
		function (reader: BcsReader) {
			return reader
				.readVec((reader) => reader.read8())
				.map((el: bigint) => String.fromCharCode(Number(el)))
				.join('');
		},
		(_str: string) => true,
	);

	bcs.registerType(
		BCS.HEX,
		function (writer: BcsWriter, data: string) {
			return writer.writeVec(Array.from(fromHEX(data)), (writer, el) => writer.write8(el));
		},
		function (reader: BcsReader) {
			let bytes = reader.readVec((reader) => reader.read8());
			return toHEX(new Uint8Array(bytes));
		},
	);

	bcs.registerType(
		BCS.BASE58,
		function (writer: BcsWriter, data: string) {
			return writer.writeVec(Array.from(fromB58(data)), (writer, el) => writer.write8(el));
		},
		function (reader: BcsReader) {
			let bytes = reader.readVec((reader) => reader.read8());
			return toB58(new Uint8Array(bytes));
		},
	);

	bcs.registerType(
		BCS.BASE64,
		function (writer: BcsWriter, data: string) {
			return writer.writeVec(Array.from(fromB64(data)), (writer, el) => writer.write8(el));
		},
		function (reader: BcsReader) {
			let bytes = reader.readVec((reader) => reader.read8());
			return toB64(new Uint8Array(bytes));
		},
	);
}

export function getRustConfig(): BcsConfig {
	return {
		genericSeparators: ['<', '>'],
		vectorType: 'Vec',
		addressLength: SUI_ADDRESS_LENGTH,
		addressEncoding: 'hex',
	};
}

export function getSuiMoveConfig(): BcsConfig {
	return {
		genericSeparators: ['<', '>'],
		vectorType: 'vector',
		addressLength: SUI_ADDRESS_LENGTH,
		addressEncoding: 'hex',
	};
}

export function splitGenericParameters(
	str: string,
	genericSeparators: [string, string] = ['<', '>'],
) {
	const [left, right] = genericSeparators;
	const tok = [];
	let word = '';
	let nestedAngleBrackets = 0;

	for (let i = 0; i < str.length; i++) {
		const char = str[i];
		if (char === left) {
			nestedAngleBrackets++;
		}
		if (char === right) {
			nestedAngleBrackets--;
		}
		if (nestedAngleBrackets === 0 && char === ',') {
			tok.push(word.trim());
			word = '';
			continue;
		}
		word += char;
	}

	tok.push(word.trim());

	return tok;
}
