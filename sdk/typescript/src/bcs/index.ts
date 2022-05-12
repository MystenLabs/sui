// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * BCS implementation {@see https://github.com/diem/bcs } for JavaScript.
 * Intended to be used for Move applications; supports both NodeJS and browser.
 *
 * For more details and examples {@see README.md }.
 *
 * @module bcs
 * @property {BcsReader}
 */

import * as BN from 'bn.js';
import { HexDataBuffer as HEX } from '../serialization/hex';
import { Base64DataBuffer as B64 } from '../serialization/base64';

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
 * let field3 = reader.read8() == '1'; // bool
 * let field4 = reader.read64();
 * // ....
 *
 * Reading vectors is another deal in BCS. To read a vector, you first need to read
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
  read8(): BN {
    let value = this.dataView.getUint8(this.bytePosition);
    this.shift(1);
    return new BN.BN(value, 10);
  }
  /**
   * Read U16 value from the buffer and shift cursor by 2.
   * @returns
   */
  read16(): BN {
    let value = this.dataView.getUint16(this.bytePosition, true);
    this.shift(2);
    return new BN.BN(value, 10);
  }
  /**
   * Read U32 value from the buffer and shift cursor by 4.
   * @returns
   */
  read32(): BN {
    let value = this.dataView.getUint32(this.bytePosition, true);
    this.shift(4);
    return new BN.BN(value, 10);
  }
  /**
   * Read U64 value from the buffer and shift cursor by 8.
   * @returns
   */
  read64(): BN {
    let value1 = this.read32();
    let value2 = this.read32();
    let result = value2.toString(16) + value1.toString(16).padStart(8, '0');

    return new BN.BN(result, 16);
  }
  /**
   * Read U128 value from the buffer and shift cursor by 16.
   * @returns
   */
  read128(): BN {
    let value1 = this.read64();
    let value2 = this.read64();
    let result = value2.toString(16) + value1.toString(16).padStart(8, '0');

    return new BN.BN(result, 16);
  }
  /**
   * Read `num` number of bytes from the buffer and shift cursor by `num`.
   * @param num Number of bytes to read.
   * @returns Selected Buffer.
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
export class BcsWriter {
  private dataView: DataView;
  private bytePosition: number = 0;

  /**
   * @param {Number} [size=1024] Size of the buffer to reserve for serialization.
   */
  constructor(size = 1024) {
    this.dataView = new DataView(new ArrayBuffer(size));
  }

  /**
   * Unify argument types by converting them to BN.
   */
  static toBN(number: number | BN | bigint | string): BN {
    switch (typeof number) {
      case 'bigint':
        return new BN.BN(number.toString());
      default:
        return new BN.BN(number);
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
  write8(value: number | bigint | BN): this {
    this.dataView.setUint8(this.bytePosition, +BcsWriter.toBN(value));
    return this.shift(1);
  }
  /**
   * Write a U16 value into a buffer and shift cursor position by 2.
   * @param {Number} value Value to write.
   * @returns {this}
   */
  write16(value: number | bigint | BN): this {
    this.dataView.setUint16(this.bytePosition, +BcsWriter.toBN(value), true);
    return this.shift(2);
  }
  /**
   * Write a U32 value into a buffer and shift cursor position by 4.
   * @param {Number} value Value to write.
   * @returns {this}
   */
  write32(value: number | bigint | BN): this {
    this.dataView.setUint32(this.bytePosition, +BcsWriter.toBN(value), true);
    return this.shift(4);
  }
  /**
   * Write a U64 value into a buffer and shift cursor position by 8.
   * @param {bigint} value Value to write.
   * @returns {this}
   */
  write64(value: bigint | BN): this {
    BcsWriter.toBN(value)
      .toArray('le', 8)
      .forEach(el => this.write8(el));

    return this;
  }
  /**
   * Write a U128 value into a buffer and shift cursor position by 16.
   *
   * @unimplemented
   * @param {bigint} value Value to write.
   * @returns {this}
   */
  write128(value: bigint | BN): this {
    BcsWriter.toBN(value)
      .toArray('le', 16)
      .forEach(el => this.write8(el));

    return this;
  }
  /**
   * Write a ULEB value into a buffer and shift cursor position by number of bytes
   * written.
   * @param {Number} value Value to write.
   * @returns {this}
   */
  writeULEB(value: number): this {
    ulebEncode(value).forEach(el => this.write8(el));
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
  writeVec(
    vector: Array<any>,
    cb: (writer: BcsWriter, el: any, i: number, len: number) => {}
  ): this {
    this.writeULEB(vector.length);
    Array.from(vector).forEach((el, i) => cb(this, el, i, vector.length));
    return this;
  }

  /**
   * Get underlying buffer taking only value bytes (in case initial buffer size was bigger).
   * @returns {Uint8Array} Resulting BCS.
   */
  toBytes() {
    return new Uint8Array(this.dataView.buffer.slice(0, this.bytePosition));
  }

  /**
   * Represent data as 'hex' or 'base64'
   * @param encoding Encoding to use: 'base64' or 'hex'
   */
  toString(encoding: string): string {
    switch (encoding) {
      case 'base64':
        return new B64(this.toBytes()).toString();
      case 'hex':
        return new HEX(this.toBytes()).toString();
      default:
        throw new Error(
          'Unsupported encoding, supported values are: base64, hex'
        );
    }
  }
}

// Helper utility: write number as an ULEB array.
// Original code is taken from: https://www.npmjs.com/package/uleb128 (no longer exists)
function ulebEncode(num: number): Array<number> {
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
function ulebDecode(
  arr: Array<number> | Uint8Array
): { value: number; length: number } {
  let total = 0;
  let shift = 0;
  let len = 0;

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
interface TypeInterface {
  encode: (data: any, size: number) => BcsWriter;
  decode: (data: Uint8Array) => any;

  _encodeRaw: (writer: BcsWriter, data: any) => BcsWriter;
  _decodeRaw: (reader: BcsReader) => any;
}

/**
 * BCS implementation for Move types and few additional built-ins.
 */
export class BCS {
  // Prefefined types constants
  static readonly U8: string = 'u8';
  static readonly U32: string = 'u32';
  static readonly U64: string = 'u64';
  static readonly U128: string = 'u128';
  static readonly BOOL: string = 'bool';
  static readonly VECTOR: string = 'vector';
  static readonly ADDRESS: string = 'address';
  static readonly STRING: string = 'string';

  private static types: Map<string, TypeInterface> = new Map();

  /**
   * Serialize data into BCS.
   *
   * @example
   * BCS.registerVectorType('vector<u8>', 'u8');
   *
   * let serialized = BCS
   *   .set('vector<u8>', [1,2,3,4,5,6])
   *   .toBytes();
   *
   * console.assert(BCS.util.toHex(serialized) === '06010203040506');
   *
   * @param type Name of the type to serialize (must be registered).
   * @param data Data to serialize.
   * @param size Serialization buffer size. Default 1024 = 1KB.
   * @return A BCS reader instance. Usually you'd want to call `.toBytes()`
   */
  public static set(type: string, data: any, size: number = 1024): BcsWriter {
    return this.getTypeInterface(type).encode(data, size);
  }

  /**
   * Deserialize BCS into a JS type.
   *
   * @example
   * // use util to form an Uint8Array buffer
   * let data = BCS.de(BCS.U32, new Uint8Array([255, 255, 255, 255]));
   * console.assert(data.toString() == '4294967295');
   *
   * @param type Name of the type to deserialize (must be registered).
   * @param data Data to deserialize.
   * @return Deserialized data.
   */
  public static de(type: string, data: Uint8Array): any {
    return this.getTypeInterface(type).decode(data);
  }

  /**
   * Check whether a TypeInterface has been loaded for the `Type`
   * @param type Name of the type to check.
   * @returns
   */
  public static hasType(type: string): boolean {
    return this.types.has(type);
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
   * BCS.registerType('number_string',
   *    (writer, data) => writer.writeVec(data, (w, el) => w.write8(el)),
   *    (reader) => reader.readVec((r) => r.read8()).join(''), // read each value as u8
   *    (value) => /[0-9]+/.test(value) // test that it has at least one digit
   * );
   * console.log(Array.from(BCS.set('number_string', '12345').toBytes()) == [5,1,2,3,4,5]);
   *
   * @param name
   * @param encodeCb Callback to encode a value.
   * @param decodeCb Callback to decode a value.
   * @param validateCb Optional validator Callback to check type before serialization.
   */
  public static registerType(
    name: string,
    encodeCb: (writer: BcsWriter, data: any) => BcsWriter,
    decodeCb: (reader: BcsReader) => any,
    validateCb: (data: any) => boolean = () => true
  ): typeof BCS {
    this.types.set(name, {
      encode(data, size = 1024) {
        return this._encodeRaw(new BcsWriter(size), data);
      },
      decode(data) {
        return this._decodeRaw(new BcsReader(data));
      },

      // these methods should always be used with caution as they require pre-defined
      // reader and writer and mainly exist to allow multi-field (de)serialization;
      _encodeRaw(writer, data) {
        if (validateCb(data)) {
          return encodeCb(writer, data);
        } else {
          throw new Error(`Validation failed for type ${name}, data: ${data}`);
        }
      },
      _decodeRaw(reader) {
        return decodeCb(reader);
      },
    });

    return this;
  }

  /**
   * Register an address type which is a sequence of U8s of specified length.
   * @example
   * BCS.registerAddressType('address', 20);
   * let addr = BCS.de('address', 'ca27601ec5d915dd40d42e36c395d4a156b24026');
   *
   * @param name Name of the address type.
   * @param length Byte length of the address.
   * @returns
   */
  public static registerAddressType(name: string, length: number): typeof BCS {
    return this.registerType(
      name,
      (writer, data) =>
        new HEX(data)
          .getData()
          .reduce((writer, el) => writer.write8(el), writer),
      reader => new HEX(reader.readBytes(length)).toString()
    );
  }

  /**
   * Register custom vector type inside the BCS.
   *
   * @example
   * BCS.registerVectorType('vector<u8>', 'u8');
   * let array = BCS.de('vector<u8>', new Uint8Array([6,1,2,3,4,5,6])); // [1,2,3,4,5,6];
   * let again = BCS.set('vector<u8>', [1,2,3,4,5,6]).toBytes();
   *
   * @param name Name of the type to register.
   * @param elementType Name of the inner type of the vector.
   * @return Returns self for chaining.
   */
  public static registerVectorType(
    name: string,
    elementType: string
  ): typeof BCS {
    // OMITTING SAFETY CHECK TO ALLOW RECURSIVE DEFINITIONS
    // if (!BCS.hasType(elementType)) {
    //     throw new Error(`Type ${elementType} is not registered`);
    // }

    return this.registerType(
      name,
      (writer, data) =>
        writer.writeVec(data, (writer, el) => {
          return BCS.getTypeInterface(elementType)._encodeRaw(writer, el);
        }),
      reader =>
        reader.readVec(reader => {
          return BCS.getTypeInterface(elementType)._decodeRaw(reader);
        })
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
   * BCS.registerStructType('Coin', {
   *   value: BCS.U64,
   *   owner: BCS.STRING,
   *   is_locked: BCS.BOOL
   * });
   *
   * // Created in Rust with diem/bcs
   * // let rust_bcs_str = '80d1b105600000000e4269672057616c6c65742047757900';
   * let rust_bcs_str = [ // using an Array here as BCS works with Uint8Buffer
   *  128, 209, 177,   5,  96,  0,  0,
   *    0,  14,  66, 105, 103, 32, 87,
   *   97, 108, 108, 101, 116, 32, 71,
   *  117, 121,   0
   * ];
   *
   * // Let's encode the value as well
   * let test_set = BCS.set('Coin', {
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
  public static registerStructType(
    name: string,
    fields: { [key: string]: string }
  ): typeof BCS {
    let struct = Object.freeze(fields); // Make sure the order doesn't get changed

    // IMPORTANT: we need to store canonical order of fields for each registered
    // struct so we maintain it and allow developers to use any field ordering in
    // their code (and not cause mismatches based on field order).
    let canonicalOrder = Object.keys(struct);

    // Make sure all the types in the fields description are already known
    // and that all the field types are strings.
    // OMITTING this check to allow recursive definitions and dynamic typing.
    // for (let type of Object.values(struct)) {
    //         throw new Error(`Type ${type} is not registered`);
    //     }
    // }

    return this.registerType(
      name,
      (writer, data) => {
        for (let key of canonicalOrder) {
          BCS.getTypeInterface(struct[key])._encodeRaw(writer, data[key]);
        }
        return writer;
      },
      reader => {
        let result: { [key: string]: any } = {};
        for (let key of canonicalOrder) {
          result[key] = BCS.getTypeInterface(struct[key])._decodeRaw(reader);
        }
        return result;
      }
    );
  }

  /**
   * Safe method to register custom enum type where each invariant holds the value of another type.
   * @example
   * BCS.registerStructType('Coin', { value: 'u64' });
   * BCS.registerVectorType('vector<Coin>', 'Coin');
   * BCS.registerEnumType('MyEnum', {
   *  single: 'Coin',
   *  multi: 'vector<Coin>'
   * });
   *
   * let example1 = Buffer.from('AICWmAAAAAAA', 'base64');
   * let example2 = Buffer.from('AQIBAAAAAAAAAAIAAAAAAAAA', 'base64');
   *
   * console.log(
   *  BCS.de('MyEnum', new Uint8Array(example1)), // { single: { value: 10000000 } }
   *  BCS.de('MyEnum', new Uint8Array(example2))  // { multi: [ { value: 1 }, { value: 2 } ] }
   * }
   *
   * // and serialization
   * BCS.set('MyEnum', { single: { value: 10000000 } }).toBytes();
   * BCS.set('MyEnum', { multi: [ { value: 1 }, { value: 2 } ] });
   *
   * @param name
   * @param variants
   */
  public static registerEnumType(
    name: string,
    variants: { [key: string]: string | null }
  ) {
    let struct = Object.freeze(variants); // Make sure the order doesn't get changed

    // IMPORTANT: enum is an ordered type and we have to preserve ordering in BCS
    let canonicalOrder = Object.keys(struct);

    return this.registerType(
      name,
      (writer, data) => {
        let key = Object.keys(data)[0];
        if (key === undefined) {
          throw new Error(`Unknown invariant of the enum ${name}`);
        }

        let orderByte = canonicalOrder.indexOf(key);
        if (orderByte === -1) {
          throw new Error(
            `Unknown invariant of the enum ${name}, allowed values: ${canonicalOrder}`
          );
        }
        let invariant = canonicalOrder[orderByte];
        let invariantType = struct[invariant];

        writer.write8(orderByte); // write order byte

        // Allow empty Enum values!
        return invariantType !== null
          ? BCS.getTypeInterface(invariantType)._encodeRaw(writer, data[key])
          : writer;
      },
      reader => {
        let orderByte = reader.readULEB();
        let invariant = canonicalOrder[orderByte];
        let invariantType = struct[invariant];

        if (orderByte === -1) {
          throw new Error(
            `Decoding type mismatch, expected enum ${name} invariant index, received ${orderByte}`
          );
        }

        return {
          [invariant]:
            invariantType !== null
              ? BCS.getTypeInterface(invariantType)._decodeRaw(reader)
              : true,
        };
      }
    );
  }

  static getTypeInterface(type: string): TypeInterface {
    let typeInterface = BCS.types.get(type);
    if (typeInterface === undefined) {
      throw new Error(`Type ${type} is not registered`);
    }
    return typeInterface;
  }
}

(function registerPrimitives(): void {
  BCS.registerType(
    BCS.U8,
    (writer, data) => writer.write8(data),
    reader => reader.read8(),
    u8 => u8 < 256
  );

  BCS.registerType(
    BCS.U32,
    (writer, data) => writer.write32(data),
    reader => reader.read32(),
    u32 => u32 < 4294967296
  );

  BCS.registerType(
    BCS.U64,
    (writer, data) => writer.write64(data),
    reader => reader.read64(),
    _u64 => true
  );

  BCS.registerType(
    BCS.U128,
    (writer, data) => writer.write128(data),
    reader => reader.read128(),
    _u128 => true
  );

  BCS.registerType(
    BCS.BOOL,
    (writer, data) => writer.write8(data),
    reader => reader.read8().toString(10) == '1',
    (_bool: boolean) => true
  );

  BCS.registerType(
    BCS.STRING,
    (writer, data) =>
      writer.writeVec(Array.from(data), (writer, el) =>
        writer.write8(el.charCodeAt(0))
      ),
    reader => {
      return reader
        .readVec(reader => reader.read8())
        .map(el => String.fromCharCode(el))
        .join('');
    },
    (str: string) => /^[\x00-\x7F]*$/.test(str)
  );
})();
