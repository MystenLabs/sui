// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ulebDecode } from './uleb';

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
